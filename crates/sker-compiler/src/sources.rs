use serde::{Deserialize, Serialize};
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct LockFile {
    pub schema_version: String,
    pub sources: Vec<LockSource>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct LockSource {
    pub kind: RuleSourceKind,
    pub source_id: String,
    pub version: String,
    pub path: String,
    pub content_hash: String,
}

#[derive(Deserialize, Default)]
struct SkerConfigFile {
    #[serde(default)]
    extends: Vec<ExtendsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ExtendsConfig {
    Path(String),
    Detailed {
        path: String,
        #[serde(default)]
        kind: Option<RuleSourceKind>,
        #[serde(default)]
        source_id: Option<String>,
        #[serde(default)]
        version: Option<String>,
    },
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn yaml<T: for<'de> Deserialize<'de>>(bytes: &[u8], path: &Path) -> Result<T, CompileError> {
    serde_yaml_ng::from_slice(bytes).map_err(|e| CompileError::BundleSerialization {
        detail: format!("{}: {e}", path.display()),
    })
}

/// Loads federated rule sources declared in root .sker.yaml `extends` configuration.
/// Manages .sker.lock creation, verification, and --update-lock refresh.
pub fn load_federated_sources(
    root: impl AsRef<Path>,
    update_lock: bool,
) -> Result<(Vec<RuleSource>, bool), CompileError> {
    let root = root.as_ref();
    let config_path = root.join(".sker.yaml");
    if !config_path.exists() {
        return Ok((Vec::new(), false));
    }
    let bytes = fs::read(&config_path).map_err(|_| CompileError::SourceUnavailable {
        source_id: config_path.display().to_string(),
    })?;
    let config: SkerConfigFile = yaml(&bytes, &config_path)?;
    if config.extends.is_empty() {
        return Ok((Vec::new(), false));
    }

    let lock_path = root.join(".sker.lock");
    let mut current_sources = Vec::new();
    let mut current_lock_sources = Vec::new();

    for ext in config.extends {
        let (rel_path, kind_opt, source_id_opt, version_opt) = match ext {
            ExtendsConfig::Path(p) => (p, None, None, None),
            ExtendsConfig::Detailed {
                path,
                kind,
                source_id,
                version,
            } => (path, kind, source_id, version),
        };

        let src_path = if Path::new(&rel_path).is_absolute() {
            PathBuf::from(&rel_path)
        } else {
            root.join(&rel_path)
        };

        if !src_path.exists() {
            return Err(CompileError::SourceUnavailable {
                source_id: rel_path,
            });
        }

        let kind = kind_opt.unwrap_or(RuleSourceKind::CentralPlaybook);
        let version = version_opt.unwrap_or_else(|| "working-tree".into());

        if src_path.is_dir() && src_path.join("sker.yaml").exists() {
            let manifest_path = src_path.join("sker.yaml");
            let m_bytes =
                fs::read(&manifest_path).map_err(|_| CompileError::SourceUnavailable {
                    source_id: manifest_path.display().to_string(),
                })?;
            let manifest: Playbook = yaml(&m_bytes, &manifest_path)?;
            let source_id = source_id_opt.unwrap_or(manifest.playbook_id);
            let mut rules = Vec::new();
            let mut all_bytes = m_bytes;
            for r_rel in manifest.rules {
                let r_path = src_path.join(&r_rel);
                let r_content = fs::read(&r_path).map_err(|_| CompileError::SourceUnavailable {
                    source_id: r_path.display().to_string(),
                })?;
                let rule: RuleEntry = yaml(&r_content, &r_path)?;
                rule.validate().map_err(|e| CompileError::InvalidEntry {
                    rule_id: Some(rule.rule_id.clone()),
                    detail: e.to_string(),
                })?;
                all_bytes.extend_from_slice(&r_content);
                rules.push(rule);
            }
            let hash = sha256_hex(&all_bytes);
            let lock = SourceLock {
                kind: kind.clone(),
                source_id: source_id.clone(),
                version: version.clone(),
                content_hash: hash.clone(),
            };
            current_sources.push(RuleSource { lock, rules });
            current_lock_sources.push(LockSource {
                kind,
                source_id,
                version,
                path: rel_path,
                content_hash: hash,
            });
        } else if src_path.is_file() {
            let content = fs::read(&src_path).map_err(|_| CompileError::SourceUnavailable {
                source_id: src_path.display().to_string(),
            })?;
            let hash = sha256_hex(&content);
            let rule: RuleEntry = yaml(&content, &src_path)?;
            rule.validate().map_err(|e| CompileError::InvalidEntry {
                rule_id: Some(rule.rule_id.clone()),
                detail: e.to_string(),
            })?;
            let source_id = source_id_opt.unwrap_or_else(|| rule.rule_id.clone());
            let lock = SourceLock {
                kind: kind.clone(),
                source_id: source_id.clone(),
                version: version.clone(),
                content_hash: hash.clone(),
            };
            current_sources.push(RuleSource {
                lock,
                rules: vec![rule],
            });
            current_lock_sources.push(LockSource {
                kind,
                source_id,
                version,
                path: rel_path,
                content_hash: hash,
            });
        } else {
            return Err(CompileError::SourceUnavailable {
                source_id: rel_path,
            });
        }
    }

    if !lock_path.exists() || update_lock {
        let lock_file = LockFile {
            schema_version: "sker.sources/v1".into(),
            sources: current_lock_sources,
        };
        let yaml_str = serde_yaml_ng::to_string(&lock_file).map_err(|e| {
            CompileError::BundleSerialization {
                detail: e.to_string(),
            }
        })?;
        fs::write(&lock_path, yaml_str).map_err(|e| CompileError::BundleSerialization {
            detail: format!("writing .sker.lock: {e}"),
        })?;
        return Ok((current_sources, true));
    }

    let lock_bytes = fs::read(&lock_path).map_err(|_| CompileError::SourceUnavailable {
        source_id: lock_path.display().to_string(),
    })?;
    let locked: LockFile = yaml(&lock_bytes, &lock_path)?;
    for curr in &current_lock_sources {
        let Some(matched) = locked
            .sources
            .iter()
            .find(|s| s.source_id == curr.source_id)
        else {
            return Err(CompileError::LockMismatch {
                source_id: format!(
                    "'{}' missing from .sker.lock; run 'sker compile --update-lock' to update",
                    curr.source_id
                ),
            });
        };
        if matched.content_hash != curr.content_hash {
            return Err(CompileError::LockMismatch {
                source_id: format!(
                    "'{}' hash mismatch (locked {}, actual {}); run 'sker compile --update-lock' to update",
                    curr.source_id, matched.content_hash, curr.content_hash
                ),
            });
        }
    }

    Ok((current_sources, true))
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
            let is_match = sha256_hex(&content) == source.content_hash
                || digest(&content) == source.content_hash;
            if !is_match {
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
