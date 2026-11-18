//! Incremental, offline analysis of repository changes.

mod baseline;
mod credential_verify;
mod diff;
mod replay;
mod rules;
mod sarif;
mod signals;

use sker_schema::{AnalysisError, AnalysisRequest, CheckReport, CheckStatus, DiffAnalyzer, RuleBundle};

#[derive(Debug, Default, Clone, Copy)]
pub struct Analyzer;

impl DiffAnalyzer for Analyzer {
    fn analyze(&self, bundle: &RuleBundle, request: &AnalysisRequest) -> Result<CheckReport, AnalysisError> {
        bundle.validate().map_err(|e| AnalysisError::RuleEvaluation { rule_id: "<bundle>".into(), detail: e.to_string() })?;
        let mut report = rules::analyze(bundle, request)?;
        report.bundle_sha256 = bundle.bundle_sha256.clone();
        report.diff_tree_hash = sker_schema::compute_diff_tree_hash(&request.diff);
        report.status = if !report.analysis.complete { CheckStatus::Incomplete } else if report.findings.iter().any(|f| f.disposition == sker_schema::FindingDisposition::Blocked) { CheckStatus::Blocked } else { CheckStatus::Passed };
        Ok(report)
    }
}

pub use diff::extract_diff;
pub use replay::{replay, ReplayCommit};
pub use sarif::{sarif_document, SARIF_VERSION};
pub use credential_verify::verify_credential;

#[cfg(test)]
mod tests {
    use super::*;
    use sker_schema::*;

    fn rule(track: RuleTrack, paths: Vec<String>, exemptions: Vec<Exemption>) -> RuleEntry {
        RuleEntry { schema_version:"sker.entry/v1".into(),rule_id:if track==RuleTrack::Deterministic{"go.no-log"}else{"semantic.signal"}.into(),version:"1".into(),title:"avoid logging".into(),domain:"logging".into(),description:"avoid logging secrets".into(),rationale:None,severity:RuleSeverity::Block,track,scope:EntryScope{include_paths:paths,exclude_paths:vec![]},signals:vec![RuleSignal{signal_id:"log".into(),languages:vec![Language::Go],description:"log call".into(),pattern:"fmt.Printf($FMT, $$$ARGS)".into()}],exemptions,overrides:vec![],fixtures:vec![EntryFixture{fixture_id:"positive".into(),polarity:FixturePolarity::Positive,path:"positive.go".into(),expected:FixtureTrigger::Trigger},EntryFixture{fixture_id:"negative".into(),polarity:FixturePolarity::Negative,path:"negative.go".into(),expected:FixtureTrigger::NoTrigger}],compliant_alternatives:Some(vec!["Use a redacted structured logger".into()]),lifecycle:Lifecycle{kind:LifecycleKind::Ordinary,cooldown_hours:Some(24),hotfix_expires_at:None},consult:ConsultMetadata{synonyms:vec![],keywords:vec![]},tags:vec![] }
    }
    fn bundle(rules:Vec<RuleEntry>)->RuleBundle { let sources=vec![];let digest=compute_bundle_sha256(&sources,&rules).unwrap();RuleBundle{schema_version:"sker.bundle/v1".into(),bundle_id:digest.clone(),bundle_sha256:digest,sources,rules,metadata:BundleMetadata{created_at:"2026-01-01T00:00:00Z".into(),compiler_version:"test".into()}} }
    fn request(path:&str, code:&str, line:u32)->AnalysisRequest { AnalysisRequest{repository_root:".".into(),base_revision:"base".into(),head_revision:"head".into(),diff:vec![DiffEntry{path:path.into(),status:DiffStatus::Modified,old_mode:Some(0o100644),new_mode:Some(0o100644),old_blob:vec![],new_blob:code.as_bytes().to_vec(),added_lines:vec![line],modified_lines:vec![]}],baseline:vec![],now:"2026-01-01T00:00:00Z".into()} }

    #[test]
    fn deterministic_findings_require_added_or_modified_lines_and_exact_scopes() {
        let b=bundle(vec![rule(RuleTrack::Deterministic,vec!["services/payments/*.go".into()],vec![])]);
        assert_eq!(Analyzer.analyze(&b,&request("services/other/main.go","package main\nfunc f(){ fmt.Printf(\"secret %s\", \"x\") }",2)).unwrap().findings.len(),0);
        let report=Analyzer.analyze(&b,&request("services/payments/main.go","package main\nfunc f(){ fmt.Printf(\"secret %s\", \"x\") }",2)).unwrap();
        assert_eq!(report.findings.len(),1);assert!(!report.findings[0].alternatives.is_empty());
        assert_eq!(Analyzer.analyze(&b,&request("services/payments/main.go","package main\nfunc f(){ fmt.Printf(\"secret %s\", \"x\") }",1)).unwrap().findings.len(),0);
    }
    #[test]
    fn exemptions_and_baseline_are_stable_when_lines_move() {
        let exempt=Exemption{exemption_id:"tests".into(),paths:vec!["**/*_test.go".into()],reason:"tests".into(),expires_at:"2099-01-01T00:00:00Z".into(),approved_by:"owner".into()};
        let b=bundle(vec![rule(RuleTrack::Deterministic,vec!["**/*.go".into()],vec![exempt])]);
        assert_eq!(Analyzer.analyze(&b,&request("pkg/foo_test.go","package pkg\nfunc f(){ fmt.Printf(\"x %s\", \"y\") }",2)).unwrap().findings.len(),0);
        let mut req=request("pkg/main.go","package pkg\n\n\nfunc f(){ fmt.Printf(\"x %s\", \"y\") }",4);req.baseline.push(BaselineItem{repository_path:"pkg/main.go".into(),rule_id:"go.no-log".into(),syntax_anchor:Some("call_expression".into()),symbol:Some("log.Printf".into()),normalized_fragment:"avoid logging".into(),line_hint:Some(1),exception:None,audit:vec![]});
        let report=Analyzer.analyze(&b,&req).unwrap();assert_eq!(report.findings[0].disposition,FindingDisposition::BaselineSuppressed);
    }
    #[test]
    fn semantic_rules_stay_silent_until_declared_signal_matches_increment() {
        let b=bundle(vec![rule(RuleTrack::Semantic,vec!["**/*.go".into()],vec![])]);
        let mut req=request("pkg/main.go","package pkg\nfunc f(){}",2);assert!(Analyzer.analyze(&b,&req).unwrap().findings.is_empty());
        req.diff[0].new_blob=b"package pkg\nfunc f(){ fmt.Printf(\"x %s\", \"y\") }".to_vec();let report=Analyzer.analyze(&b,&req).unwrap();assert_eq!(report.findings.len(),1);assert_eq!(report.findings[0].disposition,FindingDisposition::PendingSemantic);
    }
    #[test]
    fn sarif_and_replay_are_structured_and_deterministic() {
        let report=Analyzer.analyze(&bundle(vec![rule(RuleTrack::Deterministic,vec!["**/*.go".into()],vec![])]),&request("pkg/main.go","package pkg\nfunc f(){ fmt.Printf(\"x %s\", \"y\") }",2)).unwrap();
        let value=sarif_document(&report);assert_eq!(value["version"],"2.1.0");assert!(!value["runs"][0]["results"][0]["properties"]["alternatives"].as_array().unwrap().is_empty());
        let replay=replay(&[ReplayCommit{revision:"a".into(),bundle_sha256:"abc".into(),rule_ids:vec!["R".into()],complete:true}],"a","z","abc");assert_eq!(replay.rule_stats[0].trigger_count,1);
    }
    #[test]
    fn verifier_rejects_tampered_blake3_credential() {
        let mut c=AgentCredential{schema_version:"sker.credential/v1".into(),bundle_sha256:"bundle".into(),diff_tree_hash:"diff".into(),consult_trace:vec![],semantic_verdicts:vec![],issuer:CredentialIssuer::LocalCli,created_at:"2026-01-01T00:00:00Z".into(),credential_blake3:String::new()};
        c.credential_blake3=credential_blake3(&c).unwrap();assert!(verify_credential(&c,"bundle","diff",&[]).is_ok());c.bundle_sha256="changed".into();assert!(verify_credential(&c,"bundle","diff",&[]).is_err());
    }
    #[test]
    fn gix_reads_added_and_modified_lines_from_temporary_repository() {
        use std::{fs,process::Command,time::{SystemTime,UNIX_EPOCH}};
        let root=std::env::temp_dir().join(format!("sker-analyzer-{}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
        let git=|args:&[&str]|{let status=Command::new("git").args(args).current_dir(&root).status().unwrap();assert!(status.success());};
        git(&["init","-q"]);git(&["config","user.email","test@example.invalid"]);git(&["config","user.name","Analyzer Test"]);
        fs::write(root.join("sample.go"),"package sample\nfunc old() {}\n").unwrap();git(&["add","sample.go"]);git(&["commit","-qm","base"]);let base=String::from_utf8(Command::new("git").args(["rev-parse","HEAD"]).current_dir(&root).output().unwrap().stdout).unwrap().trim().to_owned();
        fs::write(root.join("sample.go"),"package sample\nfunc changed() {}\nfunc added() {}\n").unwrap();git(&["add","sample.go"]);git(&["commit","-qm","head"]);let head=String::from_utf8(Command::new("git").args(["rev-parse","HEAD"]).current_dir(&root).output().unwrap().stdout).unwrap().trim().to_owned();
        let diff=extract_diff(root.to_str().unwrap(),&base,&head).unwrap();assert_eq!(diff.len(),1);assert_eq!(diff[0].path,"sample.go");assert!(!diff[0].added_lines.is_empty());let _=fs::remove_dir_all(root);
    }
}
