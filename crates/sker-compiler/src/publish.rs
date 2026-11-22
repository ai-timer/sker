use sker_schema::{CompileError, RuleBundle, archive_bundle, validate_bundle_archive};
use std::{fs, path::Path};

/// Serializes and validates to a sibling temp file, then atomically renames it into place.
pub fn publish_bundle(
    bundle: &RuleBundle,
    destination: impl AsRef<Path>,
) -> Result<(), CompileError> {
    let destination = destination.as_ref();
    let bytes = archive_bundle(bundle)?;
    validate_bundle_archive(&bytes)?;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("bundle.rkyv");
    let temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temp, &bytes).map_err(|e| CompileError::BundleSerialization {
        detail: e.to_string(),
    })?;
    if let Err(error) = fs::rename(&temp, destination) {
        let _ = fs::remove_file(&temp);
        return Err(CompileError::BundleSerialization {
            detail: error.to_string(),
        });
    }
    Ok(())
}
pub fn write_bundle_archive(
    bundle: &RuleBundle,
    destination: impl AsRef<Path>,
) -> Result<(), CompileError> {
    publish_bundle(bundle, destination)
}
