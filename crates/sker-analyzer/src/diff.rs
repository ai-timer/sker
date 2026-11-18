use sker_schema::{AnalysisError, DiffEntry, DiffStatus};

/// Extract a deterministic tree-to-tree diff using gix's repository/object APIs.
pub fn extract_diff(root: &str, base: &str, head: &str) -> Result<Vec<DiffEntry>, AnalysisError> {
    use gix::bstr::ByteSlice;
    let repo = gix::discover(root).map_err(|e| AnalysisError::RepositoryUnavailable { detail: e.to_string() })?;
    let resolve = |rev: &str| repo.rev_parse_single(rev).map_err(|e| AnalysisError::DiffUnavailable { detail: e.to_string() });
    let old = repo.find_commit(resolve(base)?).and_then(|c| c.tree()).map_err(|e| AnalysisError::DiffUnavailable { detail: e.to_string() })?;
    let new = repo.find_commit(resolve(head)?).and_then(|c| c.tree()).map_err(|e| AnalysisError::DiffUnavailable { detail: e.to_string() })?;
    let mut platform = old.changes().map_err(|e| AnalysisError::DiffUnavailable { detail: e.to_string() })?;
    platform.options(|o| { o.track_rewrites(None); });
    let mut entries = Vec::new();
    platform.for_each_to_obtain_tree(&new, |change| {
        use gix::object::tree::diff::Change;
        let (path, status, old_mode, new_mode, old_id, new_id) = match change {
            Change::Addition { location, entry_mode, id, .. } => (location.to_str_lossy().into_owned(), DiffStatus::Added, None, Some(entry_mode.value() as u32), None, Some(id)),
            Change::Deletion { location, entry_mode, id, .. } => (location.to_str_lossy().into_owned(), DiffStatus::Deleted, Some(entry_mode.value() as u32), None, Some(id), None),
            Change::Modification { location, previous_entry_mode, previous_id, entry_mode, id, .. } => (location.to_str_lossy().into_owned(), DiffStatus::Modified, Some(previous_entry_mode.value() as u32), Some(entry_mode.value() as u32), Some(previous_id), Some(id)),
            Change::Rewrite { .. } => return Ok(std::ops::ControlFlow::Continue(())),
        };
        let old_blob = old_id.and_then(|id| id.object().ok()).map(|o| o.data.clone()).unwrap_or_default();
        let new_blob = new_id.and_then(|id| id.object().ok()).map(|o| o.data.clone()).unwrap_or_default();
        let (added_lines, modified_lines) = line_changes(&old_blob, &new_blob);
        entries.push(DiffEntry { path, status, old_mode, new_mode, old_blob, new_blob, added_lines, modified_lines });
        Ok(std::ops::ControlFlow::Continue(()))
    }).map_err(|e| AnalysisError::DiffUnavailable { detail: e.to_string() })?;
    entries.sort_by(|a,b| a.path.cmp(&b.path));
    Ok(entries)
}

fn line_changes(old: &[u8], new: &[u8]) -> (Vec<u32>, Vec<u32>) {
    let a: Vec<_> = old.split(|b| *b == b'\n').collect(); let b: Vec<_> = new.split(|b| *b == b'\n').collect();
    let prefix = a.iter().zip(&b).take_while(|(left,right)| left == right).count();
    let mut suffix = 0;
    while suffix < a.len().saturating_sub(prefix) && suffix < b.len().saturating_sub(prefix)
        && a[a.len()-1-suffix] == b[b.len()-1-suffix] { suffix += 1; }
    let new_end = b.len().saturating_sub(suffix);
    let old_middle = a.len().saturating_sub(prefix + suffix);
    let new_middle = new_end.saturating_sub(prefix);
    let added: Vec<_> = (prefix..new_end).map(|i| (i+1) as u32).collect();
    let modified: Vec<_> = (prefix..prefix + old_middle.min(new_middle)).map(|i| (i+1) as u32).collect();
    (added, modified)
}
