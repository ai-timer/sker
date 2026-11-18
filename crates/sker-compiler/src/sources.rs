use serde::Deserialize;
use sker_schema::{CompileError, RuleEntry, RuleSource, RuleSourceKind, SourceLock};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Playbook {
    schema_version: String,
    playbook_id: String,
    rules: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockFile {
    schema_version: String,
    sources: Vec<LockSource>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockSource {
    kind: RuleSourceKind,
    source_id: String,
    version: String,
    path: String,
    content_hash: String,
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn yaml<T: for<'de> Deserialize<'de>>(bytes: &[u8], path: &Path) -> Result<T, CompileError> {
    serde_yaml_ng::from_slice(bytes).map_err(|e| CompileError::BundleSerialization {
        detail: format!("{}: {e}", path.display()),
    })
}

/// Loads a playbook and all referenced entries, resolving paths beneath `root` only.
pub fn load_playbook(root: impl AsRef<Path>) -> Result<RuleSource, CompileError> {
    let root = root.as_ref();
    let manifest_path = root.join("sker.yaml");
    let bytes = fs::read(&manifest_path).map_err(|_| CompileError::SourceUnavailable {
        source_id: manifest_path.display().to_string(),
    })?;
    let manifest: Playbook = yaml(&bytes, &manifest_path)?;
    if manifest.schema_version != "sker.playbook/v1"
        || manifest.playbook_id.is_empty()
        || manifest.rules.is_empty()
    {
        return Err(CompileError::BundleSerialization {
            detail: "invalid playbook manifest".into(),
        });
    }
    let mut rules = Vec::new();
    let mut all_bytes = bytes;
    for relative in manifest.rules {
        let path = root.join(&relative);
        if !path.starts_with(root) {
            return Err(CompileError::SourceUnavailable {
                source_id: relative,
            });
        }
        let content = fs::read(&path).map_err(|_| CompileError::SourceUnavailable {
            source_id: path.display().to_string(),
        })?;
        let rule: RuleEntry = yaml(&content, &path)?;
        rule.validate().map_err(|e| CompileError::InvalidEntry {
            rule_id: Some(rule.rule_id.clone()),
            detail: e.to_string(),
        })?;
        all_bytes.extend_from_slice(&content);
        rules.push(rule);
    }
    Ok(RuleSource {
        lock: SourceLock {
            kind: RuleSourceKind::CentralPlaybook,
            source_id: manifest.playbook_id,
            version: "working-tree".into(),
            content_hash: digest(&all_bytes),
        },
        rules,
    })
}

/// Loads a lock file whose pinned source hashes are checked before parsing rules.
pub fn load_sources(
    root: impl AsRef<Path>,
    lock_path: impl AsRef<Path>,
) -> Result<Vec<RuleSource>, CompileError> {
    let root = root.as_ref();
    let lock_path = lock_path.as_ref();
    let bytes = fs::read(lock_path).map_err(|_| CompileError::SourceUnavailable {
        source_id: lock_path.display().to_string(),
    })?;
    let lock: LockFile = yaml(&bytes, lock_path)?;
    if lock.schema_version != "sker.sources/v1" {
        return Err(CompileError::BundleSerialization {
            detail: "invalid source lock schema".into(),
        });
    }
    lock.sources
        .into_iter()
        .map(|source| {
            let path = root.join(&source.path);
            if !path.starts_with(root) {
                return Err(CompileError::SourceUnavailable {
                    source_id: source.source_id,
                });
            }
            let content = fs::read(&path).map_err(|_| CompileError::SourceUnavailable {
                source_id: source.source_id.clone(),
            })?;
            if digest(&content) != source.content_hash {
                return Err(CompileError::LockMismatch {
                    source_id: source.source_id,
                });
            }
            let rules: Vec<RuleEntry> =
                if source.path.ends_with(".yaml") || source.path.ends_with(".yml") {
                    vec![yaml(&content, &path)?]
                } else {
                    serde_yaml_ng::from_slice(&content).map_err(|e| {
                        CompileError::BundleSerialization {
                            detail: e.to_string(),
                        }
                    })?
                };
            Ok(RuleSource {
                lock: SourceLock {
                    kind: source.kind,
                    source_id: source.source_id,
                    version: source.version,
                    content_hash: source.content_hash,
                },
                rules,
            })
        })
        .collect()
}

#[allow(dead_code)]
fn _pathbuf(_: PathBuf) {}
