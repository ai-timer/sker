use sker_schema::{AnalysisError, DiffEntry, DiffStatus};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

pub const EMPTY_TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub entries: Vec<DiffEntry>,
    pub base_revision: String,
    pub head_revision: String,
}

pub fn is_sker_internal_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let p = p.trim_start_matches("./");
    p.starts_with(".sker/bundle.")
        || p == ".sker/baseline.json"
        || p == ".sker/credential.json"
        || p.starts_with(".sker/cache/")
        || p.starts_with(".sker/tmp/")
        || p.starts_with(".sker/report.")
        || p.ends_with(".sarif")
        || p == "sker-report.json"
}

/// Compute incremental diff range following Sker's strict governance rules:
/// - Explicit `--base <rev>`: diff `<rev>..HEAD` (overlays working tree in local mode); errors on invalid rev.
/// - Local `sker check` (no `--ci`, no `--base`):
///   1) Detect workspace staged + unstaged + untracked non-ignored files (skipping internal sker artifacts).
///   2) If workspace has local changes, diff HEAD (or empty-tree if unborn) against working tree.
///   3) If workspace is clean, fall back to `parent..HEAD` (or empty-tree if root commit).
/// - CI mode (`--ci`, no `--base`): diff `parent..HEAD`; falls back safely to empty tree if root or shallow.
pub fn compute_diff(root: &str, base: Option<&str>, ci: bool) -> Result<DiffResult, AnalysisError> {
    if let Some(rev) = base {
        // Explicit base revision specified
        let check_out = Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--verify", rev])
            .output()
            .map_err(|e| AnalysisError::DiffUnavailable {
                detail: format!("failed to invoke git: {e}"),
            })?;
        if !check_out.status.success() {
            return Err(AnalysisError::DiffUnavailable {
                detail: format!("unknown or invalid base revision '{rev}'"),
            });
        }
        let resolved_base = String::from_utf8_lossy(&check_out.stdout)
            .trim()
            .to_string();

        if ci {
            let entries = extract_diff(root, &resolved_base, "HEAD")?;
            return Ok(DiffResult {
                entries,
                base_revision: rev.to_string(),
                head_revision: "HEAD".to_string(),
            });
        } else {
            // Local mode with explicit base: overlay working tree changes if any
            let tree_entries = extract_diff(root, &resolved_base, "HEAD").unwrap_or_default();
            let dirty_files = get_workspace_changed_files(root)?;
            if dirty_files.is_empty() {
                return Ok(DiffResult {
                    entries: tree_entries,
                    base_revision: rev.to_string(),
                    head_revision: "HEAD".to_string(),
                });
            }

            // Overlay working tree changes on top of base diff
            let mut entry_map: std::collections::BTreeMap<String, DiffEntry> = tree_entries
                .into_iter()
                .map(|e| (e.path.clone(), e))
                .collect();

            for path in dirty_files {
                let full_path = Path::new(root).join(&path);
                let (new_blob, new_mode) = if full_path.is_file() {
                    (fs::read(&full_path).unwrap_or_default(), Some(0o100644))
                } else {
                    (Vec::new(), None)
                };

                let (old_blob, old_mode) = get_blob_at_rev(root, &resolved_base, &path);
                if old_blob == new_blob {
                    entry_map.remove(&path);
                    continue;
                }

                let status = if old_blob.is_empty() && !new_blob.is_empty() {
                    DiffStatus::Added
                } else if !old_blob.is_empty() && new_blob.is_empty() {
                    DiffStatus::Deleted
                } else {
                    DiffStatus::Modified
                };

                let (added_lines, modified_lines) = line_changes(&old_blob, &new_blob);
                entry_map.insert(
                    path.clone(),
                    DiffEntry {
                        path,
                        status,
                        old_mode,
                        new_mode,
                        old_blob,
                        new_blob,
                        added_lines,
                        modified_lines,
                    },
                );
            }

            let mut entries: Vec<DiffEntry> = entry_map.into_values().collect();
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            return Ok(DiffResult {
                entries,
                base_revision: rev.to_string(),
                head_revision: "working-tree".to_string(),
            });
        }
    }

    // No explicit base: check if HEAD exists
    let head_check = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map_err(|e| AnalysisError::DiffUnavailable {
            detail: format!("failed to invoke git: {e}"),
        })?;
    let head_exists = head_check.status.success();

    if !ci {
        // Local mode: detect working tree modifications/untracked files
        let dirty_files = get_workspace_changed_files(root)?;
        if !dirty_files.is_empty() {
            // Working tree has changes: scan working tree against HEAD (or empty tree if unborn)
            let base_rev = if head_exists { "HEAD" } else { EMPTY_TREE_SHA };

            let mut entries = Vec::new();
            for path in dirty_files {
                let full_path = Path::new(root).join(&path);
                let (new_blob, new_mode) = if full_path.is_file() {
                    (fs::read(&full_path).unwrap_or_default(), Some(0o100644))
                } else {
                    (Vec::new(), None)
                };

                let (old_blob, old_mode) = if head_exists {
                    get_blob_at_rev(root, base_rev, &path)
                } else {
                    (Vec::new(), None)
                };

                if old_blob == new_blob {
                    continue;
                }

                let status = if old_blob.is_empty() && !new_blob.is_empty() {
                    DiffStatus::Added
                } else if !old_blob.is_empty() && new_blob.is_empty() {
                    DiffStatus::Deleted
                } else {
                    DiffStatus::Modified
                };

                let (added_lines, modified_lines) = line_changes(&old_blob, &new_blob);
                entries.push(DiffEntry {
                    path,
                    status,
                    old_mode,
                    new_mode,
                    old_blob,
                    new_blob,
                    added_lines,
                    modified_lines,
                });
            }
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            return Ok(DiffResult {
                entries,
                base_revision: if head_exists {
                    "HEAD".into()
                } else {
                    "empty-tree".into()
                },
                head_revision: "working-tree".into(),
            });
        }

        // Workspace is completely clean: fall back to parent..HEAD or empty tree if root commit
        if !head_exists {
            return Ok(DiffResult {
                entries: Vec::new(),
                base_revision: "empty-tree".into(),
                head_revision: "HEAD".into(),
            });
        }

        let parent_check = Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--verify", "HEAD^"])
            .output()
            .map_err(|e| AnalysisError::DiffUnavailable {
                detail: format!("failed to invoke git: {e}"),
            })?;

        if parent_check.status.success() {
            let parent_rev = String::from_utf8_lossy(&parent_check.stdout)
                .trim()
                .to_string();
            let entries = extract_diff(root, &parent_rev, "HEAD")?;
            return Ok(DiffResult {
                entries,
                base_revision: parent_rev,
                head_revision: "HEAD".into(),
            });
        } else {
            // Root commit (no parent): diff empty tree vs HEAD safely
            let entries = extract_diff(root, EMPTY_TREE_SHA, "HEAD")?;
            return Ok(DiffResult {
                entries,
                base_revision: "empty-tree".into(),
                head_revision: "HEAD".into(),
            });
        }
    }

    // CI mode (ci: true, base: None)
    if !head_exists {
        return Ok(DiffResult {
            entries: Vec::new(),
            base_revision: "empty-tree".into(),
            head_revision: "HEAD".into(),
        });
    }

    let parent_check = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", "HEAD^"])
        .output()
        .map_err(|e| AnalysisError::DiffUnavailable {
            detail: format!("failed to invoke git: {e}"),
        })?;

    if parent_check.status.success() {
        let parent_rev = String::from_utf8_lossy(&parent_check.stdout)
            .trim()
            .to_string();
        let entries = extract_diff(root, &parent_rev, "HEAD")?;
        Ok(DiffResult {
            entries,
            base_revision: parent_rev,
            head_revision: "HEAD".into(),
        })
    } else {
        // Root commit or shallow clone missing parent: safe empty tree fallback
        let entries = extract_diff(root, EMPTY_TREE_SHA, "HEAD")?;
        Ok(DiffResult {
            entries,
            base_revision: "empty-tree".into(),
            head_revision: "HEAD".into(),
        })
    }
}

pub fn get_workspace_changed_files(root: &str) -> Result<Vec<String>, AnalysisError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain=v1", "-z", "-uall"])
        .output()
        .map_err(|e| AnalysisError::DiffUnavailable {
            detail: format!("git status error: {e}"),
        })?;

    if !output.status.success() {
        return Err(AnalysisError::DiffUnavailable {
            detail: format!(
                "git status failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let mut changed = BTreeSet::new();
    let bytes = &output.stdout;
    let mut i = 0;
    while i < bytes.len() {
        let end = bytes[i..]
            .iter()
            .position(|&b| b == 0)
            .map(|pos| i + pos)
            .unwrap_or(bytes.len());
        let item = &bytes[i..end];
        i = end + 1;
        if item.len() < 3 {
            continue;
        }

        let status_code = &item[0..2];
        let path_bytes = &item[3..];
        let path = String::from_utf8_lossy(path_bytes).into_owned();

        let actual_path = if status_code.contains(&b'R') || status_code.contains(&b'C') {
            if i < bytes.len() {
                let next_end = bytes[i..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|pos| i + pos)
                    .unwrap_or(bytes.len());
                let dest_bytes = &bytes[i..next_end];
                i = next_end + 1;
                String::from_utf8_lossy(dest_bytes).into_owned()
            } else {
                path
            }
        } else {
            path
        };

        if !is_sker_internal_path(&actual_path) {
            changed.insert(actual_path);
        }
    }

    Ok(changed.into_iter().collect())
}

fn get_blob_at_rev(root: &str, rev: &str, path: &str) -> (Vec<u8>, Option<u32>) {
    let output = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{rev}:{path}")])
        .output();
    match output {
        Ok(out) if out.status.success() => (out.stdout, Some(0o100644)),
        _ => (Vec::new(), None),
    }
}

/// Extract a deterministic tree-to-tree diff using gix's repository/object APIs.
pub fn extract_diff(root: &str, base: &str, head: &str) -> Result<Vec<DiffEntry>, AnalysisError> {
    use gix::bstr::ByteSlice;
    let repo = gix::open_opts(root, gix::open::Options::isolated())
        .or_else(|_| gix::discover(root))
        .map_err(|e| AnalysisError::RepositoryUnavailable {
            detail: e.to_string(),
        })?;

    let is_empty_tree = |rev: &str| rev == "empty-tree" || rev == EMPTY_TREE_SHA;

    let old_tree = if is_empty_tree(base) {
        repo.empty_tree()
    } else {
        let id = repo
            .rev_parse_single(base)
            .map_err(|e| AnalysisError::DiffUnavailable {
                detail: format!("resolving base '{base}': {e}"),
            })?;
        repo.find_commit(id)
            .and_then(|c| c.tree())
            .map_err(|e| AnalysisError::DiffUnavailable {
                detail: format!("finding base commit tree '{base}': {e}"),
            })?
    };

    let new_tree = if is_empty_tree(head) {
        repo.empty_tree()
    } else {
        let id = repo
            .rev_parse_single(head)
            .map_err(|e| AnalysisError::DiffUnavailable {
                detail: format!("resolving head '{head}': {e}"),
            })?;
        repo.find_commit(id)
            .and_then(|c| c.tree())
            .map_err(|e| AnalysisError::DiffUnavailable {
                detail: format!("finding head commit tree '{head}': {e}"),
            })?
    };

    let mut platform = old_tree
        .changes()
        .map_err(|e| AnalysisError::DiffUnavailable {
            detail: e.to_string(),
        })?;
    platform.options(|o| {
        o.track_rewrites(None);
    });
    let mut entries = Vec::new();
    platform
        .for_each_to_obtain_tree(&new_tree, |change| {
            use gix::object::tree::diff::Change;
            let (path, status, old_mode, new_mode, old_id, new_id) = match change {
                Change::Addition {
                    location,
                    entry_mode,
                    id,
                    ..
                } => {
                    if entry_mode.is_tree() {
                        return Ok(std::ops::ControlFlow::Continue(()));
                    }
                    (
                        location.to_str_lossy().into_owned(),
                        DiffStatus::Added,
                        None,
                        Some(entry_mode.value() as u32),
                        None,
                        Some(id),
                    )
                }
                Change::Deletion {
                    location,
                    entry_mode,
                    id,
                    ..
                } => {
                    if entry_mode.is_tree() {
                        return Ok(std::ops::ControlFlow::Continue(()));
                    }
                    (
                        location.to_str_lossy().into_owned(),
                        DiffStatus::Deleted,
                        Some(entry_mode.value() as u32),
                        None,
                        Some(id),
                        None,
                    )
                }
                Change::Modification {
                    location,
                    previous_entry_mode,
                    previous_id,
                    entry_mode,
                    id,
                    ..
                } => {
                    if entry_mode.is_tree() || previous_entry_mode.is_tree() {
                        return Ok(std::ops::ControlFlow::Continue(()));
                    }
                    (
                        location.to_str_lossy().into_owned(),
                        DiffStatus::Modified,
                        Some(previous_entry_mode.value() as u32),
                        Some(entry_mode.value() as u32),
                        Some(previous_id),
                        Some(id),
                    )
                }
                Change::Rewrite { .. } => return Ok(std::ops::ControlFlow::Continue(())),
            };
            let old_blob = old_id
                .and_then(|id| id.object().ok())
                .map(|o| o.data.clone())
                .unwrap_or_default();
            let new_blob = new_id
                .and_then(|id| id.object().ok())
                .map(|o| o.data.clone())
                .unwrap_or_default();
            let (added_lines, modified_lines) = line_changes(&old_blob, &new_blob);
            entries.push(DiffEntry {
                path,
                status,
                old_mode,
                new_mode,
                old_blob,
                new_blob,
                added_lines,
                modified_lines,
            });
            Ok(std::ops::ControlFlow::Continue(()))
        })
        .map_err(|e| AnalysisError::DiffUnavailable {
            detail: e.to_string(),
        })?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn line_changes(old: &[u8], new: &[u8]) -> (Vec<u32>, Vec<u32>) {
    let a: Vec<_> = old.split(|b| *b == b'\n').collect();
    let b: Vec<_> = new.split(|b| *b == b'\n').collect();
    let prefix = a
        .iter()
        .zip(&b)
        .take_while(|(left, right)| left == right)
        .count();
    let mut suffix = 0;
    while suffix < a.len().saturating_sub(prefix)
        && suffix < b.len().saturating_sub(prefix)
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let new_end = b.len().saturating_sub(suffix);
    let old_middle = a.len().saturating_sub(prefix + suffix);
    let new_middle = new_end.saturating_sub(prefix);
    let added: Vec<_> = (prefix..new_end).map(|i| (i + 1) as u32).collect();
    let modified: Vec<_> = (prefix..prefix + old_middle.min(new_middle))
        .map(|i| (i + 1) as u32)
        .collect();
    (added, modified)
}
