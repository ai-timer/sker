//! Offline BM25 retrieval over an immutable rule bundle.
use globset::{Glob, GlobSet, GlobSetBuilder};
pub use sker_schema::IntentRetriever;
use sker_schema::{ConsultQuery, ConsultResponse, ConsultResultStatus, RetrievalError, RuleBundle, RuleEntry, RuleMatch, SearchQuery, SearchResponse, SearchStatus, compute_bundle_sha256};
use tantivy::{Index, TantivyDocument, doc, schema::{Schema, STORED, TEXT, Value as TantivyValue}, query::QueryParser, collector::TopDocs};

pub struct TantivyRetriever { minimum_score: f64 }

impl Default for TantivyRetriever { fn default() -> Self { Self { minimum_score: 0.08 } } }

impl TantivyRetriever {
    pub fn new(minimum_score: f64) -> Self { Self { minimum_score: minimum_score.clamp(0.0, 1.0) } }

    pub fn ranked_search(&self, bundle: &RuleBundle, query: &str, paths: &[String], domains: &[String], limit: usize) -> Result<Vec<RuleMatch>, RetrievalError> {
        if query.trim().is_empty() { return Err(RetrievalError::InvalidQuery { detail: "query must not be empty".into() }); }
        sker_schema::validate_query_paths(paths)?;
        self.retrieve(bundle, query, paths, domains, limit).map(|hits| hits.into_iter().map(|(index, score)| as_match(&bundle.rules[index], score)).collect())
    }

    fn retrieve(&self, bundle: &RuleBundle, query: &str, paths: &[String], domains: &[String], limit: usize) -> Result<Vec<(usize, f64)>, RetrievalError> {
        let mut builder = Schema::builder();
        let id = builder.add_u64_field("id", STORED);
        let text = builder.add_text_field("text", TEXT);
        let domain = builder.add_text_field("domain", TEXT | STORED);
        let path = builder.add_text_field("path", TEXT | STORED);
        let schema = builder.build();
        let index = Index::create_in_ram(schema);
        let mut writer = index.writer(15_000_000).map_err(|e| RetrievalError::IndexUnavailable { detail: e.to_string() })?;
        for (i, rule) in bundle.rules.iter().enumerate() {
            let expanded = format!("{} {} {} {} {} {}", rule.title, rule.description, rule.domain, rule.consult.keywords.join(" "), rule.consult.synonyms.join(" "), rule.tags.join(" "));
            for scope in &rule.scope.include_paths {
                writer.add_document(doc!(id=>i as u64, text=>expanded.clone(), domain=>rule.domain.clone(), path=>scope.clone())).map_err(|e| RetrievalError::IndexUnavailable { detail: e.to_string() })?;
            }
        }
        writer.commit().map_err(|e| RetrievalError::IndexUnavailable { detail: e.to_string() })?;
        let reader = index.reader().map_err(|e| RetrievalError::IndexUnavailable { detail: e.to_string() })?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(&index, vec![text]);
        let parsed = parser.parse_query(query).map_err(|e| RetrievalError::InvalidQuery { detail: e.to_string() })?;
        let hits = searcher.search(&parsed, &TopDocs::with_limit(bundle.rules.len().saturating_mul(8).max(1)).order_by_score()).map_err(|e| RetrievalError::IndexCorrupt { detail: e.to_string() })?;
        let mut ranked = std::collections::BTreeMap::<usize, f64>::new();
        for (score, address) in hits {
            let doc: TantivyDocument = searcher.doc(address).map_err(|e| RetrievalError::IndexCorrupt { detail: e.to_string() })?;
            let Some(value) = doc.get_first(id).and_then(|v| v.as_u64()) else { continue };
            let idx = value as usize;
            let Some(rule) = bundle.rules.get(idx) else { continue };
            if !domains.is_empty() && !domains.iter().any(|d| d == &rule.domain) { continue; }
            let scopes = compile_paths(&rule.scope.include_paths)?;
            if !paths.is_empty() && !paths.iter().any(|p| scopes.iter().any(|glob| glob.is_match(p))) { continue; }
            ranked.entry(idx).and_modify(|old| *old = old.max(score as f64)).or_insert(score as f64);
        }
        let mut output: Vec<_> = ranked.into_iter().collect();
        let best = output.iter().map(|(_, score)| *score).fold(0.0_f64, f64::max);
        if best > 0.0 { for (_, score) in &mut output { *score = (*score / best * 0.95).clamp(0.0, 0.95); } }
        output.sort_by(|a,b| b.1.total_cmp(&a.1).then_with(|| bundle.rules[a.0].rule_id.cmp(&bundle.rules[b.0].rule_id)));
        output.truncate(limit.clamp(1, 100));
        Ok(output)
    }
}

fn compile_paths(paths: &[String]) -> Result<Vec<GlobSet>, RetrievalError> {
    paths.iter().map(|pattern| { let mut b = GlobSetBuilder::new(); let glob = Glob::new(pattern).map_err(|e| RetrievalError::InvalidQuery { detail: e.to_string() })?; b.add(glob); b.build().map_err(|e| RetrievalError::InvalidQuery { detail: e.to_string() }) }).collect()
}
fn as_match(rule: &RuleEntry, score: f64) -> RuleMatch { RuleMatch { rule_id: rule.rule_id.clone(), title: rule.title.clone(), domain: rule.domain.clone(), severity: rule.severity, track: rule.track, paths: rule.scope.include_paths.clone(), summary: rule.description.clone(), score } }
fn checksum(bundle: &RuleBundle) -> String { compute_bundle_sha256(&bundle.sources, &bundle.rules).unwrap_or_else(|_| bundle.bundle_sha256.clone()) }

impl IntentRetriever for TantivyRetriever {
    fn consult(&self, bundle: &RuleBundle, query: &ConsultQuery) -> Result<ConsultResponse, RetrievalError> {
        if query.intent.trim().is_empty() || query.paths.is_empty() { return Err(RetrievalError::InvalidQuery { detail: "intent and at least one path are required".into() }); }
        sker_schema::validate_query_paths(&query.paths)?;
        let ranked = self.retrieve(bundle, &query.intent, &query.paths, &[], query.limit.unwrap_or(10))?;
        let threshold = query.minimum_score.unwrap_or(self.minimum_score).clamp(0.0, 1.0);
        let matches: Vec<_> = ranked.into_iter().filter(|(_,score)| *score >= threshold).map(|(i,score)| as_match(&bundle.rules[i], score)).collect();
        Ok(ConsultResponse { status: if matches.is_empty() { ConsultResultStatus::NoMatch } else { ConsultResultStatus::Matched }, matches, bundle_sha256: checksum(bundle), trace: None })
    }
    fn search(&self, bundle: &RuleBundle, query: &SearchQuery) -> Result<SearchResponse, RetrievalError> {
        if query.query.trim().is_empty() { return Err(RetrievalError::InvalidQuery { detail: "query must not be empty".into() }); }
        sker_schema::validate_query_paths(&query.paths)?;
        let ranked = self.ranked_search(bundle, &query.query, &query.paths, &query.domains, query.limit.unwrap_or(20))?;
        let rules: Vec<_> = ranked.into_iter().map(|matched| bundle.rules.iter().find(|rule| rule.rule_id == matched.rule_id).expect("retriever returns bundle rule IDs").clone()).collect();
        Ok(SearchResponse { status: if rules.is_empty() { SearchStatus::NoMatch } else { SearchStatus::Matched }, rules, bundle_sha256: checksum(bundle) })
    }
}
