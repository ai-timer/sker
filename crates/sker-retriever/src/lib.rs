//! Offline BM25 retrieval over an immutable rule bundle.
use globset::{Glob, GlobSet, GlobSetBuilder};
pub use sker_schema::IntentRetriever;
use sker_schema::{
    ConsultQuery, ConsultResponse, ConsultResultStatus, RetrievalError, RuleBundle, RuleEntry,
    RuleMatch, SearchQuery, SearchResponse, SearchStatus, compute_bundle_sha256,
};
use tantivy::{
    Index, TantivyDocument,
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{STORED, Schema, TEXT, Value as TantivyValue},
    tokenizer::{Token, TokenStream, Tokenizer},
};

#[derive(Clone, Default)]
pub struct CjkWordsTokenizer;

pub struct CjkWordsTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl TokenStream for CjkWordsTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }
    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

impl Tokenizer for CjkWordsTokenizer {
    type TokenStream<'a> = CjkWordsTokenStream;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let mut tokens = Vec::new();
        let mut pos = 0;
        let mut word_buf = String::new();
        let mut word_start = 0;
        let mut cjk_chars: Vec<(char, usize, usize)> = Vec::new();

        let flush_word = |word_buf: &mut String,
                          word_start: usize,
                          pos: &mut usize,
                          tokens: &mut Vec<Token>,
                          byte_end: usize| {
            if !word_buf.is_empty() {
                tokens.push(Token {
                    offset_from: word_start,
                    offset_to: byte_end,
                    position: *pos,
                    text: word_buf.to_lowercase(),
                    position_length: 1,
                });
                *pos += 1;
                word_buf.clear();
            }
        };

        let flush_cjk = |cjk_chars: &mut Vec<(char, usize, usize)>,
                         pos: &mut usize,
                         tokens: &mut Vec<Token>| {
            if cjk_chars.is_empty() {
                return;
            }
            // 1. Unigrams
            for (ch, start, end) in cjk_chars.iter() {
                tokens.push(Token {
                    offset_from: *start,
                    offset_to: *end,
                    position: *pos,
                    text: ch.to_string(),
                    position_length: 1,
                });
                *pos += 1;
            }
            // 2. Bigrams
            for i in 0..cjk_chars.len().saturating_sub(1) {
                let (_, start, _) = cjk_chars[i];
                let (_, _, end) = cjk_chars[i + 1];
                let mut bigram = String::new();
                bigram.push(cjk_chars[i].0);
                bigram.push(cjk_chars[i + 1].0);
                tokens.push(Token {
                    offset_from: start,
                    offset_to: end,
                    position: *pos,
                    text: bigram,
                    position_length: 1,
                });
                *pos += 1;
            }
            cjk_chars.clear();
        };

        let mut byte_offset = 0;
        for ch in text.chars() {
            let ch_len = ch.len_utf8();
            let next_offset = byte_offset + ch_len;

            if is_cjk(ch) {
                flush_word(
                    &mut word_buf,
                    word_start,
                    &mut pos,
                    &mut tokens,
                    byte_offset,
                );
                cjk_chars.push((ch, byte_offset, next_offset));
            } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                flush_cjk(&mut cjk_chars, &mut pos, &mut tokens);
                if word_buf.is_empty() {
                    word_start = byte_offset;
                }
                word_buf.push(ch);
            } else {
                flush_word(
                    &mut word_buf,
                    word_start,
                    &mut pos,
                    &mut tokens,
                    byte_offset,
                );
                flush_cjk(&mut cjk_chars, &mut pos, &mut tokens);
            }
            byte_offset = next_offset;
        }
        flush_word(
            &mut word_buf,
            word_start,
            &mut pos,
            &mut tokens,
            byte_offset,
        );
        flush_cjk(&mut cjk_chars, &mut pos, &mut tokens);

        CjkWordsTokenStream { tokens, index: 0 }
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4e00}'..='\u{9fff}' |
        '\u{3400}'..='\u{4dbf}' |
        '\u{20000}'..='\u{2a6df}' |
        '\u{f900}'..='\u{faff}'
    )
}

fn normalize_path(p: &str) -> String {
    let mut s = p.trim();
    while let Some(stripped) = s.strip_prefix("./") {
        s = stripped;
    }
    while let Some(stripped) = s.strip_prefix('/') {
        s = stripped;
    }
    s.to_string()
}

pub struct TantivyRetriever {
    minimum_score: f64,
}

impl Default for TantivyRetriever {
    fn default() -> Self {
        Self {
            minimum_score: 0.08,
        }
    }
}

impl TantivyRetriever {
    pub fn new(minimum_score: f64) -> Self {
        Self {
            minimum_score: minimum_score.clamp(0.0, 1.0),
        }
    }

    pub fn ranked_search(
        &self,
        bundle: &RuleBundle,
        query: &str,
        paths: &[String],
        domains: &[String],
        limit: usize,
    ) -> Result<Vec<RuleMatch>, RetrievalError> {
        if query.trim().is_empty() {
            return Err(RetrievalError::InvalidQuery {
                detail: "query must not be empty".into(),
            });
        }
        sker_schema::validate_query_paths(paths)?;
        self.retrieve(bundle, query, paths, domains, limit)
            .map(|hits| {
                hits.into_iter()
                    .map(|(index, score)| as_match(&bundle.rules[index], score))
                    .collect()
            })
    }

    fn retrieve(
        &self,
        bundle: &RuleBundle,
        query: &str,
        paths: &[String],
        domains: &[String],
        limit: usize,
    ) -> Result<Vec<(usize, f64)>, RetrievalError> {
        let mut builder = Schema::builder();
        let id = builder.add_u64_field("id", STORED);
        let text_options = tantivy::schema::TextOptions::default()
            .set_indexing_options(
                tantivy::schema::TextFieldIndexing::default()
                    .set_tokenizer("cjk_words")
                    .set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();
        let text = builder.add_text_field("text", text_options);
        let domain = builder.add_text_field("domain", TEXT | STORED);
        let path = builder.add_text_field("path", TEXT | STORED);
        let schema = builder.build();
        let index = Index::create_in_ram(schema);
        index.tokenizers().register("cjk_words", CjkWordsTokenizer);

        let mut writer =
            index
                .writer(15_000_000)
                .map_err(|e| RetrievalError::IndexUnavailable {
                    detail: e.to_string(),
                })?;
        for (i, rule) in bundle.rules.iter().enumerate() {
            let expanded = format!(
                "{} {} {} {} {} {}",
                rule.title,
                rule.description,
                rule.domain,
                rule.consult.keywords.join(" "),
                rule.consult.synonyms.join(" "),
                rule.tags.join(" ")
            );
            for scope in &rule.scope.include_paths {
                writer.add_document(doc!(id=>i as u64, text=>expanded.clone(), domain=>rule.domain.clone(), path=>scope.clone())).map_err(|e| RetrievalError::IndexUnavailable { detail: e.to_string() })?;
            }
        }
        writer
            .commit()
            .map_err(|e| RetrievalError::IndexUnavailable {
                detail: e.to_string(),
            })?;
        let reader = index
            .reader()
            .map_err(|e| RetrievalError::IndexUnavailable {
                detail: e.to_string(),
            })?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(&index, vec![text]);
        let parsed = match parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => {
                let safe_query: String = query
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || is_cjk(c) || c.is_whitespace() {
                            c
                        } else {
                            ' '
                        }
                    })
                    .collect();
                parser
                    .parse_query(safe_query.trim())
                    .map_err(|e| RetrievalError::InvalidQuery {
                        detail: e.to_string(),
                    })?
            }
        };
        let hits = searcher
            .search(
                &parsed,
                &TopDocs::with_limit(bundle.rules.len().saturating_mul(8).max(1)).order_by_score(),
            )
            .map_err(|e| RetrievalError::IndexCorrupt {
                detail: e.to_string(),
            })?;
        let normalized_paths: Vec<String> = paths.iter().map(|p| normalize_path(p)).collect();
        let mut ranked = std::collections::BTreeMap::<usize, f64>::new();
        for (score, address) in hits {
            let doc: TantivyDocument =
                searcher
                    .doc(address)
                    .map_err(|e| RetrievalError::IndexCorrupt {
                        detail: e.to_string(),
                    })?;
            let Some(value) = doc.get_first(id).and_then(|v| v.as_u64()) else {
                continue;
            };
            let idx = value as usize;
            let Some(rule) = bundle.rules.get(idx) else {
                continue;
            };
            if !domains.is_empty() && !domains.iter().any(|d| d == &rule.domain) {
                continue;
            }
            let scopes = compile_paths(&rule.scope.include_paths)?;
            if !normalized_paths.is_empty()
                && !normalized_paths.iter().any(|p| {
                    scopes
                        .iter()
                        .any(|glob| glob.is_match(p) || glob.is_match(format!("./{p}")))
                })
            {
                continue;
            }
            ranked
                .entry(idx)
                .and_modify(|old| *old = old.max(score as f64))
                .or_insert(score as f64);
        }
        let mut output: Vec<_> = ranked.into_iter().collect();
        let best = output
            .iter()
            .map(|(_, score)| *score)
            .fold(0.0_f64, f64::max);
        if best > 0.0 {
            for (_, score) in &mut output {
                *score = (*score / best * 0.95).clamp(0.0, 0.95);
            }
        }
        output.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| bundle.rules[a.0].rule_id.cmp(&bundle.rules[b.0].rule_id))
        });
        output.truncate(limit.clamp(1, 100));
        Ok(output)
    }
}

fn compile_paths(paths: &[String]) -> Result<Vec<GlobSet>, RetrievalError> {
    paths
        .iter()
        .map(|pattern| {
            let mut b = GlobSetBuilder::new();
            let glob = Glob::new(pattern).map_err(|e| RetrievalError::InvalidQuery {
                detail: e.to_string(),
            })?;
            b.add(glob);
            if let Some(stripped) = pattern.strip_prefix("**/")
                && let Ok(fallback_glob) = Glob::new(stripped)
            {
                b.add(fallback_glob);
            }
            b.build().map_err(|e| RetrievalError::InvalidQuery {
                detail: e.to_string(),
            })
        })
        .collect()
}
fn as_match(rule: &RuleEntry, score: f64) -> RuleMatch {
    RuleMatch {
        rule_id: rule.rule_id.clone(),
        title: rule.title.clone(),
        domain: rule.domain.clone(),
        severity: rule.severity,
        track: rule.track,
        paths: rule.scope.include_paths.clone(),
        summary: rule.description.clone(),
        score,
    }
}
fn checksum(bundle: &RuleBundle) -> String {
    compute_bundle_sha256(&bundle.sources, &bundle.rules)
        .unwrap_or_else(|_| bundle.bundle_sha256.clone())
}

impl IntentRetriever for TantivyRetriever {
    fn consult(
        &self,
        bundle: &RuleBundle,
        query: &ConsultQuery,
    ) -> Result<ConsultResponse, RetrievalError> {
        if query.intent.trim().is_empty() || query.paths.is_empty() {
            return Err(RetrievalError::InvalidQuery {
                detail: "intent and at least one path are required".into(),
            });
        }
        sker_schema::validate_query_paths(&query.paths)?;
        let ranked = self.retrieve(
            bundle,
            &query.intent,
            &query.paths,
            &[],
            query.limit.unwrap_or(10),
        )?;
        let threshold = query
            .minimum_score
            .unwrap_or(self.minimum_score)
            .clamp(0.0, 1.0);
        let matches: Vec<_> = ranked
            .into_iter()
            .filter(|(_, score)| *score >= threshold)
            .map(|(i, score)| as_match(&bundle.rules[i], score))
            .collect();
        Ok(ConsultResponse {
            status: if matches.is_empty() {
                ConsultResultStatus::NoMatch
            } else {
                ConsultResultStatus::Matched
            },
            matches,
            bundle_sha256: checksum(bundle),
            trace: None,
        })
    }
    fn search(
        &self,
        bundle: &RuleBundle,
        query: &SearchQuery,
    ) -> Result<SearchResponse, RetrievalError> {
        if query.query.trim().is_empty() {
            return Err(RetrievalError::InvalidQuery {
                detail: "query must not be empty".into(),
            });
        }
        sker_schema::validate_query_paths(&query.paths)?;
        let ranked = self.ranked_search(
            bundle,
            &query.query,
            &query.paths,
            &query.domains,
            query.limit.unwrap_or(20),
        )?;
        let rules: Vec<_> = ranked
            .into_iter()
            .map(|matched| {
                bundle
                    .rules
                    .iter()
                    .find(|rule| rule.rule_id == matched.rule_id)
                    .expect("retriever returns bundle rule IDs")
                    .clone()
            })
            .collect();
        Ok(SearchResponse {
            status: if rules.is_empty() {
                SearchStatus::NoMatch
            } else {
                SearchStatus::Matched
            },
            rules,
            bundle_sha256: checksum(bundle),
        })
    }
}
