use sker_schema::{ReplayReport, ReplayRuleStats};
#[derive(Debug, Clone)] pub struct ReplayCommit { pub revision:String, pub bundle_sha256:String, pub rule_ids:Vec<String>, pub complete:bool }
pub fn replay(commits:&[ReplayCommit], from:&str,to:&str,bundle_sha256:&str)->ReplayReport {
 let selected:Vec<_>=commits.iter().filter(|c|c.revision.as_str()>=from&&c.revision.as_str()<=to&&c.bundle_sha256==bundle_sha256).collect();
 let mut counts=std::collections::BTreeMap::<String,(u64,u64)>::new(); for c in &selected {for id in &c.rule_ids {let x=counts.entry(id.clone()).or_default();x.0+=1;x.1+=1;}}
 let total=selected.len() as u64; let stats=counts.into_iter().map(|(rule_id,(trigger_count,commit_count))| ReplayRuleStats{rule_id,trigger_count,commit_count,trigger_rate:if total==0 {None}else{Some(commit_count as f64/total as f64)},potential_false_positive_samples:Vec::new()}).collect();
 ReplayReport{from:from.into(),to:to.into(),bundle_sha256:bundle_sha256.into(),complete:selected.iter().all(|c|c.complete)&&!selected.is_empty(),missing_history_reason:if selected.is_empty(){Some("no matching history in requested window/bundle".into())}else{None},rule_stats:stats}
}
