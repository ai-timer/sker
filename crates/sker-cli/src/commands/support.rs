use anyhow::{Context, Result, bail};
use sker_schema::{RuleBundle, RuleEntry};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn find_repo(path: &Path) -> Result<PathBuf> {
    let p = fs::canonicalize(path)
        .with_context(|| format!("repository not found: {}", path.display()))?;
    if !p.is_dir() {
        bail!("repository path is not a directory")
    };
    Ok(p)
}
pub fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}
pub fn load_rules(repo: &Path) -> Result<Vec<RuleEntry>> {
    let mut files = Vec::new();
    let rules = repo.join(".sker/rules");
    if rules.exists() {
        collect_rules(&rules, &mut files)?;
    }
    if files.is_empty() && repo.join("sker.yaml").exists() {
        let cfg: serde_yaml_ng::Value =
            serde_yaml_ng::from_slice(&fs::read(repo.join("sker.yaml"))?)?;
        if let Some(list) = cfg.get("rules").and_then(|v| v.as_sequence()) {
            for p in list {
                if let Some(s) = p.as_str() {
                    files.push(repo.join(s));
                }
            }
        }
    }
    let mut out = Vec::new();
    for f in files {
        let b = fs::read(&f).with_context(|| format!("reading {}", f.display()))?;
        out.push(
            serde_yaml_ng::from_slice(&b)
                .with_context(|| format!("parsing rule {}", f.display()))?,
        );
    }
    Ok(out)
}
fn collect_rules(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for e in fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            collect_rules(&p, files)?
        } else if p.extension().is_some_and(|x| x == "yaml" || x == "yml") {
            files.push(p)
        }
    }
    files.sort();
    Ok(())
}
pub fn bundle(repo: &Path) -> Result<RuleBundle> {
    let path = repo.join(".sker/bundle.json");
    let b: RuleBundle = serde_json::from_slice(
        &fs::read(&path)
            .with_context(|| format!("missing bundle {}; run `sker compile`", path.display()))?,
    )?;
    b.validate()?;
    Ok(b)
}
pub fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("report");
    let tmp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let data = serde_json::to_vec_pretty(value)?;
    let result = (|| {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(&data)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
pub fn load_baseline(repo: &Path) -> Result<Vec<sker_schema::BaselineItem>> {
    let p = repo.join(".sker/baseline.json");
    if !p.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
