use sker_schema::{BaselineItem, CheckFinding};

#[derive(Debug, Clone)]
pub(crate) struct MatchAnchor {
    pub syntax_anchor: Option<String>,
    pub symbol: Option<String>,
    pub norm: String,
    pub hash: String,
}

pub(crate) fn apply(findings: &mut [(CheckFinding, MatchAnchor)], baseline: &[BaselineItem]) {
    let mut consumed = vec![false; baseline.len()];
    for (finding, anchor) in findings.iter_mut() {
        if finding.disposition == sker_schema::FindingDisposition::BaselineSuppressed {
            continue;
        }
        let candidate_indices: Vec<usize> = baseline
            .iter()
            .enumerate()
            .filter(|(idx, item)| {
                let is_message_match = item.normalized_fragment == normalize(&finding.message)
                    || item.normalized_fragment == finding.message;
                let is_ast_match = (item.syntax_anchor.is_none()
                    || item.syntax_anchor.as_deref() == anchor.syntax_anchor.as_deref())
                    && (item.symbol.is_none()
                        || item.symbol.as_deref() == anchor.symbol.as_deref()
                        || anchor
                            .norm
                            .contains(item.symbol.as_deref().unwrap_or("___")))
                    && (item.normalized_fragment == anchor.hash
                        || item.normalized_fragment == anchor.norm);
                !consumed[*idx]
                    && item.repository_path == finding.path
                    && item.rule_id == finding.rule_id
                    && (is_ast_match || is_message_match)
            })
            .map(|(idx, _)| idx)
            .collect();

        if candidate_indices.len() == 1 {
            let chosen = candidate_indices[0];
            consumed[chosen] = true;
            finding.disposition = sker_schema::FindingDisposition::BaselineSuppressed;
            finding.baseline_anchor = baseline[chosen]
                .syntax_anchor
                .clone()
                .or_else(|| baseline[chosen].symbol.clone())
                .or_else(|| anchor.syntax_anchor.clone());
        } else if candidate_indices.len() > 1 {
            finding.message.push_str(" (ambiguous baseline identity)");
        }
    }
}

pub(crate) fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
