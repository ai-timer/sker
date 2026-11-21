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
    if p.is_file()
        && let Some(parent) = p.parent()
    {
        return Ok(parent.to_path_buf());
    }
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
    // Check .sker.yaml first (standard project root config), then fallback to sker.yaml (playbook manifest)
    let cfg_file = if repo.join(".sker.yaml").exists() {
        Some(repo.join(".sker.yaml"))
    } else if repo.join("sker.yaml").exists() {
        Some(repo.join("sker.yaml"))
    } else {
        None
    };
    if let Some(cfg_path) = cfg_file {
        let cfg: serde_yaml_ng::Value = serde_yaml_ng::from_slice(
            &fs::read(&cfg_path).with_context(|| format!("reading {}", cfg_path.display()))?,
        )?;
        if let Some(list) = cfg.get("rules").and_then(|v| v.as_sequence()) {
            for p in list {
                if let Some(s) = p.as_str() {
                    let rule_path = repo.join(s);
                    if !files.contains(&rule_path) {
                        files.push(rule_path);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for f in files {
        let b = fs::read(&f).with_context(|| format!("reading {}", f.display()))?;
        let mut entry: RuleEntry = serde_yaml_ng::from_slice(&b)
            .with_context(|| format!("parsing rule {}", f.display()))?;
        let det_ref = entry
            .detector
            .as_deref()
            .or(entry.ast_rule.as_deref())
            .or_else(|| {
                entry
                    .signals
                    .iter()
                    .find_map(|s| s.detector.as_deref().or(s.ast_rule.as_deref()))
            });
        if let Some(det_path) = det_ref
            && entry.detector_content.is_none()
        {
            let p1 = repo.join(det_path);
            let p2 = f.parent().map(|p| p.join(det_path));
            let p3 = f
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join(det_path));
            let content = if p1.exists() {
                fs::read_to_string(&p1).ok()
            } else if let Some(ref p) = p2.filter(|p| p.exists()) {
                fs::read_to_string(p).ok()
            } else if let Some(ref p) = p3.filter(|p| p.exists()) {
                fs::read_to_string(p).ok()
            } else if det_path.contains('\n')
                || (det_path.contains(':')
                    && !det_path.ends_with(".yaml")
                    && !det_path.ends_with(".yml"))
            {
                Some(det_path.to_string())
            } else {
                None
            };
            if let Some(c) = content {
                entry.detector_content = Some(c);
            }
        }
        out.push(entry);
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
    bundle_with_override(repo, None)
}

pub fn bundle_with_override(repo: &Path, bundle_override: Option<&Path>) -> Result<RuleBundle> {
    if let Some(p) = bundle_override {
        if p.is_file() {
            if p.extension().is_some_and(|ext| ext == "json") {
                let b: RuleBundle = serde_json::from_slice(&fs::read(p)?)?;
                b.validate()?;
                let expected_hash = sker_schema::compute_bundle_sha256(&b.sources, &b.rules)?;
                if b.bundle_sha256 != expected_hash {
                    bail!(
                        "bundle sha256 mismatch: recorded {}, computed {}",
                        b.bundle_sha256,
                        expected_hash
                    );
                }
                return Ok(b);
            }
            let bytes =
                fs::read(p).with_context(|| format!("reading bundle archive {}", p.display()))?;
            let b = sker_schema::validate_bundle_archive(&bytes)
                .map_err(|e| anyhow::anyhow!("corrupt bundle archive: {e}"))?;
            let expected_hash = sker_schema::compute_bundle_sha256(&b.sources, &b.rules)
                .map_err(|e| anyhow::anyhow!("bundle sha256 computation failed: {e}"))?;
            if b.bundle_sha256 != expected_hash {
                bail!(
                    "bundle archive sha256 mismatch: recorded {}, computed {}",
                    b.bundle_sha256,
                    expected_hash
                );
            }
            return Ok(b);
        } else if p.is_dir() {
            let rkyv_path = p.join("bundle.rkyv");
            if rkyv_path.exists() {
                return bundle_with_override(repo, Some(&rkyv_path));
            }
            let json_path = p.join("bundle.json");
            if json_path.exists() {
                return bundle_with_override(repo, Some(&json_path));
            }
        }
    }
    let rkyv_path = repo.join(".sker/bundle.rkyv");
    if rkyv_path.exists() {
        let bytes = fs::read(&rkyv_path)
            .with_context(|| format!("reading bundle archive {}", rkyv_path.display()))?;
        let b = sker_schema::validate_bundle_archive(&bytes)
            .map_err(|e| anyhow::anyhow!("corrupt bundle archive: {e}"))?;
        let expected_hash = sker_schema::compute_bundle_sha256(&b.sources, &b.rules)
            .map_err(|e| anyhow::anyhow!("bundle sha256 computation failed: {e}"))?;
        if b.bundle_sha256 != expected_hash {
            bail!(
                "bundle archive sha256 mismatch: recorded {}, computed {}",
                b.bundle_sha256,
                expected_hash
            );
        }
        return Ok(b);
    }
    let json_path = repo.join(".sker/bundle.json");
    if json_path.exists() {
        let b: RuleBundle = serde_json::from_slice(
            &fs::read(&json_path)
                .with_context(|| format!("reading bundle {}", json_path.display()))?,
        )?;
        b.validate()?;
        let expected_hash = sker_schema::compute_bundle_sha256(&b.sources, &b.rules)
            .map_err(|e| anyhow::anyhow!("bundle sha256 computation failed: {e}"))?;
        if b.bundle_sha256 != expected_hash {
            bail!(
                "bundle sha256 mismatch: recorded {}, computed {}",
                b.bundle_sha256,
                expected_hash
            );
        }
        return Ok(b);
    }
    bail!(
        "missing bundle at {} or {}; run `sker compile`",
        rkyv_path.display(),
        json_path.display()
    )
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
