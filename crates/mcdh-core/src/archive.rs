use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::{CoreError, Result};

pub(crate) fn is_supported_archive(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("zip" | "mcpack" | "mcaddon")
    )
}

pub(crate) fn extract_archive(source: &Path, destination: &Path) -> Result<()> {
    let file = File::open(source).map_err(|error| CoreError::io(source, error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| archive_error(source, error))?;
    fs::create_dir_all(destination).map_err(|error| CoreError::io(destination, error))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| archive_error(source, error))?;
        let enclosed = entry.enclosed_name().ok_or_else(|| CoreError::Archive {
            path: source.to_path_buf(),
            message: format!("条目包含不安全路径：{}", entry.name()),
        })?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(CoreError::Archive {
                path: source.to_path_buf(),
                message: format!("不允许符号链接条目：{}", entry.name()),
            });
        }

        let output = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| CoreError::io(&output, error))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| CoreError::io(parent, error))?;
        }
        let mut file = File::create(&output).map_err(|error| CoreError::io(&output, error))?;
        io::copy(&mut entry, &mut file).map_err(|error| CoreError::io(&output, error))?;
    }
    Ok(())
}

pub(crate) fn expand_nested_mcpacks(root: &Path) -> Result<()> {
    for _ in 0..4 {
        let packages = nested_packages(root)?;
        if packages.is_empty() {
            return Ok(());
        }
        for package in packages {
            let parent = package.parent().unwrap_or(root);
            let stem = package
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.trim().is_empty())
                .unwrap_or("package");
            let destination = unique_directory(parent, stem);
            extract_archive(&package, &destination)?;
            fs::remove_file(&package).map_err(|error| CoreError::io(&package, error))?;
        }
    }
    if nested_packages(root)?.is_empty() {
        Ok(())
    } else {
        Err(CoreError::Archive {
            path: root.to_path_buf(),
            message: "嵌套包层级超过限制".into(),
        })
    }
}

pub(crate) fn write_zip(source: &Path, destination: &Path) -> Result<()> {
    let file = File::create(destination).map_err(|error| CoreError::io(destination, error))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| {
            CoreError::io(
                error.path().unwrap_or(source),
                io::Error::other(error.to_string()),
            )
        })?;
        if entry.path() == source || entry.file_type().is_symlink() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| CoreError::InvalidInput("无法计算压缩包相对路径".into()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            writer
                .add_directory(format!("{name}/"), options)
                .map_err(|error| archive_error(destination, error))?;
        } else if entry.file_type().is_file() {
            writer
                .start_file(&name, options)
                .map_err(|error| archive_error(destination, error))?;
            let mut input =
                File::open(entry.path()).map_err(|error| CoreError::io(entry.path(), error))?;
            io::copy(&mut input, &mut writer).map_err(|error| CoreError::io(destination, error))?;
        }
    }
    writer
        .finish()
        .map_err(|error| archive_error(destination, error))?;
    Ok(())
}

fn nested_packages(root: &Path) -> Result<Vec<PathBuf>> {
    let mut packages = Vec::new();
    for entry in WalkDir::new(root).max_depth(6).follow_links(false) {
        let entry = entry.map_err(|error| {
            CoreError::io(
                error.path().unwrap_or(root),
                io::Error::other(error.to_string()),
            )
        })?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mcpack"))
        {
            packages.push(entry.path().to_path_buf());
        }
    }
    packages.sort();
    Ok(packages)
}

fn unique_directory(parent: &Path, stem: &str) -> PathBuf {
    let first = parent.join(stem);
    if !first.exists() {
        return first;
    }
    for suffix in 2.. {
        let candidate = parent.join(format!("{stem} ({suffix})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn archive_error(path: &Path, error: impl std::fmt::Display) -> CoreError {
    CoreError::Archive {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

pub(crate) fn temporary_zip_path(parent: &Path) -> PathBuf {
    parent.join(format!(".mcdh-export-{}.zip", Uuid::new_v4().simple()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_parent_directory_entries() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("unsafe.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("../escaped.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"unsafe").unwrap();
        writer.finish().unwrap();

        let destination = temp.path().join("extract");
        let error = extract_archive(&archive_path, &destination).unwrap_err();
        assert_eq!(error.code(), "archive_error");
        assert!(!temp.path().join("escaped.txt").exists());
    }
}
