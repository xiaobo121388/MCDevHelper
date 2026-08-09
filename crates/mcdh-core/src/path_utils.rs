use std::fs;
use std::path::{Path, PathBuf};

use crate::{CoreError, Result};

pub(crate) fn canonicalize(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|error| CoreError::io(path, error))?;
    Ok(strip_verbatim_prefix(canonical))
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    if let Some(ordinary) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(ordinary);
    }
    path
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_paths_are_suitable_for_display() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = canonicalize(temp.path()).unwrap();
        #[cfg(windows)]
        assert!(!canonical.to_string_lossy().starts_with(r"\\?\"));
        assert!(canonical.is_absolute());
    }
}
