use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_language::{Go, Python, Tsx, Yaml};
use globset::Glob;
use sker_schema::{AnalysisError, AnalysisRequest, AnalysisSummary, CheckFinding, CheckReport, CheckStatus, FindingDisposition, RuleBundle, RuleSeverity, RuleTrack};

pub(crate) fn analyze(bundle:&RuleBundle,request:&AnalysisRequest)->Result<CheckReport,AnalysisError>{
 let mut findings=Vec::new();let mut unsupported=Vec::new();let mut changed=0;
 for file in &request.diff {let lines:Vec<_>=file.added_lines.iter().chain(&file.modified_lines).copied().collect();changed+=lines.len() as u64;if lines.is_empty(){continue} let src=String::from_utf8_lossy(&file.new_blob);let ext=file.path.rsplit('.').next().unwrap_or("");
  let found=match ext {"go"=>check_lang(Go,&src,file,&lines,bundle,&mut findings),"py"=>check_lang(Python,&src,file,&lines,bundle,&mut findings),"ts"|"tsx"|"js"|"jsx"=>check_lang(Tsx,&src,file,&lines,bundle,&mut findings),"yaml"|"yml"=>check_lang(Yaml,&src,file,&lines,bundle,&mut findings),_=>false};if !found{unsupported.push(file.path.clone())}
 }
 findings.extend(crate::signals::extract(bundle,request));crate::baseline::apply(&mut findings,&request.baseline);findings.sort_by(|a,b|(&a.path,a.start_line,&a.rule_id).cmp(&(&b.path,b.start_line,&b.rule_id)));
 let complete=unsupported.is_empty();Ok(CheckReport{schema_version:"sker.check-report/v1".into(),status:if complete{CheckStatus::Passed}else{CheckStatus::Incomplete},bundle_sha256:String::new(),diff_tree_hash:String::new(),changed_line_count:changed,findings,analysis:AnalysisSummary{complete,unsupported_paths:unsupported,warnings:Vec::new()}})
}
fn check_lang<L>(lang:L,src:&str,file:&sker_schema::DiffEntry,lines:&[u32],bundle:&RuleBundle,out:&mut Vec<CheckFinding>)->bool where L:ast_grep_core::Language+LanguageExt+Clone {
 let root=lang.ast_grep(src);
 for rule in bundle.rules.iter().filter(|r|r.track==RuleTrack::Deterministic&&path_matches(&r.scope.include_paths,&r.scope.exclude_paths,&file.path)&&!r.exemptions.iter().any(|e|e.paths.iter().any(|p|glob(p,&file.path)))) {
  for sig in &rule.signals {if !sig.languages.iter().any(|l|matches!(l,sker_schema::Language::Any)||matches!((l,file.path.rsplit('.').next().unwrap_or("")),(sker_schema::Language::Go,"go")|(sker_schema::Language::Python,"py")|(sker_schema::Language::Typescript,"ts"|"tsx"|"js"|"jsx")|(sker_schema::Language::Yaml|sker_schema::Language::Infra,"yaml"|"yml"))){continue}let pattern=ast_grep_core::Pattern::new(&sig.pattern,lang.clone());for n in root.root().find_all(pattern){let start=n.start_pos().line() as u32+1;let end=n.end_pos().line() as u32+1;if !lines.iter().any(|l|*l>=start&&*l<=end){continue} out.push(CheckFinding{rule_id:rule.rule_id.clone(),path:file.path.clone(),start_line:start,end_line:end,track:rule.track,severity:rule.severity,disposition:if rule.severity==RuleSeverity::Block{FindingDisposition::Blocked}else{FindingDisposition::Warning},message:rule.title.clone(),alternatives:rule.compliant_alternatives.clone().unwrap_or_default(),baseline_anchor:None});}}
 } true
}
pub(crate) fn glob(p:&str,path:&str)->bool{Glob::new(p).ok().is_some_and(|g|g.compile_matcher().is_match(path))}
pub(crate) fn path_matches(include:&[String],exclude:&[String],path:&str)->bool{include.iter().any(|p|glob(p,path))&&!exclude.iter().any(|p|glob(p,path))}
