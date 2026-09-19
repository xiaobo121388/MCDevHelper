use crate::{CoreError, Result};
use fs2::FileExt;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempDir};
use walkdir::WalkDir;

pub(super) fn failure(code: &'static str, message: impl Into<String>) -> CoreError {
    CoreError::CustomExport {
        code,
        message: message.into(),
    }
}

pub(super) fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(super) fn directory(path: &Path) -> Result<PathBuf> {
    let result = crate::path_utils::canonicalize(path)?;
    if !result.is_dir() {
        return Err(CoreError::InvalidInput("路径不是目录".into()));
    }
    Ok(result)
}

pub(super) fn outside(source: &Path, destination: &Path) -> Result<()> {
    let key = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let (source, destination) = (key(source), key(destination));
    if destination == source || destination.starts_with(&(source + "\\")) {
        return Err(CoreError::InvalidInput(
            "导出目录或临时目录不能位于源组件内部".into(),
        ));
    }
    Ok(())
}

pub(super) struct HostWorkspace {
    pub root: TempDir,
    _owner: File,
}

impl HostWorkspace {
    pub fn create(index: &crate::LocalIndex) -> Result<Self> {
        let _guard = index.try_lock_mutations()?;
        let parent = index.path().parent().unwrap().join("custom-export-jobs");
        fs::create_dir_all(&parent).map_err(|e| CoreError::io(&parent, e))?;
        // The exclusive lease survives every worker. Only dead owners' directories are reclaimed.
        for entry in fs::read_dir(&parent)
            .map_err(|e| CoreError::io(&parent, e))?
            .flatten()
        {
            if !entry.file_name().to_string_lossy().starts_with("host-") {
                continue;
            }
            let path = entry.path();
            if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir() && !is_link(&m)) {
                continue;
            }
            let lease = path.join("owner.lock");
            if !fs::symlink_metadata(&lease).is_ok_and(|m| m.is_file() && !is_link(&m)) {
                continue;
            }
            if let Ok(file) = OpenOptions::new().read(true).write(true).open(&lease)
                && file.try_lock_exclusive().is_ok()
            {
                let _ = fs::remove_dir_all(&path);
            }
        }
        let root = tempfile::Builder::new()
            .prefix("host-")
            .tempdir_in(&parent)
            .map_err(|e| CoreError::io(&parent, e))?;
        let lease = root.path().join("owner.lock");
        let owner = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(&lease)
            .map_err(|e| CoreError::io(&lease, e))?;
        owner
            .lock_exclusive()
            .map_err(|e| CoreError::io(&lease, e))?;
        Ok(Self {
            root,
            _owner: owner,
        })
    }
}

pub(super) fn copy_snapshot(
    source: &Path,
    target: &Path,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    for entry in WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| fs::symlink_metadata(entry.path()).map_or(true, |m| !is_link(&m)))
    {
        check()?;
        let entry = entry.map_err(|e| CoreError::io(source, std::io::Error::other(e)))?;
        let output = target.join(entry.path().strip_prefix(source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&output).map_err(|e| CoreError::io(&output, e))?;
        } else if entry.file_type().is_file() {
            let mut input = File::open(entry.path()).map_err(|e| CoreError::io(entry.path(), e))?;
            let mut file = File::create(&output).map_err(|e| CoreError::io(&output, e))?;
            copy_checked(&mut input, &mut file, check)?;
        }
    }
    Ok(())
}

pub(super) fn copy_checked(
    input: &mut File,
    output: &mut File,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    let mut buffer = vec![0; 256 * 1024];
    loop {
        check()?;
        let read = input
            .read(&mut buffer)
            .map_err(|e| CoreError::io("export input", e))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|e| CoreError::io("export output", e))?;
    }
    Ok(())
}

pub(super) fn valid_filename(name: &str) -> bool {
    if name.is_empty()
        || name.ends_with(['.', ' '])
        || name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap().trim_end().to_uppercase();
    !matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) && !(stem.starts_with("COM") || stem.starts_with("LPT"))
        .then(|| &stem[3..])
        .is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

pub(super) fn artifact(output: &Path) -> Result<PathBuf> {
    let invalid = || {
        failure(
            "invalid_artifact",
            "产物目录必须恰好包含一个非空普通文件，不允许子目录或链接",
        )
    };
    let root = fs::symlink_metadata(output).map_err(|_| invalid())?;
    if !root.is_dir() || is_link(&root) {
        return Err(invalid());
    }
    let paths = fs::read_dir(output)
        .map_err(|_| invalid())?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| invalid())?;
    if paths.len() != 1 {
        return Err(invalid());
    }
    let path = paths[0].path();
    let metadata = fs::symlink_metadata(&path).map_err(|_| invalid())?;
    if !metadata.is_file()
        || metadata.len() == 0
        || is_link(&metadata)
        || !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(valid_filename)
    {
        return Err(invalid());
    }
    Ok(path)
}

pub(super) fn numbered(path: &Path, suffix: u64) -> PathBuf {
    let stem = path.file_stem().unwrap().to_string_lossy();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    path.with_file_name(format!("{stem} ({suffix}){extension}"))
}

pub(super) fn prepare_publish(
    artifact: &Path,
    destination: &Path,
    check: &impl Fn() -> Result<()>,
) -> Result<NamedTempFile> {
    let mut temporary = tempfile::Builder::new()
        .prefix(".mcdh-export-")
        .tempfile_in(destination)
        .map_err(|e| failure("publish_failed", e.to_string()))?;
    let mut input = File::open(artifact).map_err(|e| failure("publish_failed", e.to_string()))?;
    copy_checked(&mut input, temporary.as_file_mut(), check)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| failure("publish_failed", e.to_string()))?;
    Ok(temporary)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_and_single_artifacts() {
        for name in ["NUL.zip", "COM1", "LPT¹.x", "bad.", "a:b", "a/b", "x "] {
            assert!(!valid_filename(name), "{name}");
        }
        assert!(valid_filename("中文包.custom"));
        let temp = tempfile::tempdir().unwrap();
        assert!(artifact(temp.path()).is_err());
        fs::write(temp.path().join("out.bin"), b"x").unwrap();
        assert!(artifact(temp.path()).is_ok());
        fs::create_dir(temp.path().join("extra")).unwrap();
        assert!(artifact(temp.path()).is_err());
        assert_eq!(
            numbered(Path::new("a.tar.gz"), 2),
            PathBuf::from("a.tar (2).gz")
        );
    }
}
