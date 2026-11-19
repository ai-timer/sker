use sker_schema::{BaselineItem, CheckFinding};

pub(crate) fn apply(findings: &mut [CheckFinding], baseline: &[BaselineItem]) {
    for finding in findings {
        if finding.disposition == sker_schema::FindingDisposition::BaselineSuppressed {
            continue;
        }
        let candidates: Vec<_> = baseline
            .iter()
            .filter(|item| {
                item.repository_path == finding.path
                    && item.rule_id == finding.rule_id
                    && (item.normalized_fragment == normalize(&finding.message)
                        || item.normalized_fragment == finding.message)
            })
            .collect();
        if candidates.len() == 1 {
            finding.disposition = sker_schema::FindingDisposition::BaselineSuppressed;
            finding.baseline_anchor = candidates[0]
                .syntax_anchor
                .clone()
                .or_else(|| candidates[0].symbol.clone());
        } else if candidates.len() > 1 {
            finding.message.push_str(" (ambiguous baseline identity)");
        }
    }
}
pub(crate) fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
