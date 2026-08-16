use std::fs::{self, File};
use std::io::{self, Write};
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
    write_zip_mappings(
        &[(source.to_path_buf(), PathBuf::new())],
        destination,
        false,
        None,
    )
}

pub(crate) fn write_zip_with_extra_file(
    source: &Path,
    destination: &Path,
    name: &str,
    content: &[u8],
) -> Result<()> {
    write_zip_mappings(
        &[(source.to_path_buf(), PathBuf::new())],
        destination,
        false,
        Some((name, content)),
    )
}

pub(crate) fn write_addon_zip_roots(sources: &[PathBuf], destination: &Path) -> Result<()> {
    let mappings = sources
        .iter()
        .map(|source| {
            let prefix = source
                .file_name()
                .map(PathBuf::from)
                .ok_or_else(|| CoreError::InvalidComponent(source.clone()))?;
            Ok((source.clone(), prefix))
        })
        .collect::<Result<Vec<_>>>()?;
    write_zip_mappings(&mappings, destination, true, None)
}

fn write_zip_mappings(
    mappings: &[(PathBuf, PathBuf)],
    destination: &Path,
    exclude_python_artifacts: bool,
    extra_file: Option<(&str, &[u8])>,
) -> Result<()> {
    let file = File::create(destination).map_err(|error| CoreError::io(destination, error))?;
    let mut writer = ZipWriter::new(file);
    let compressed_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(1));
    let stored_options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    for (source, prefix) in mappings {
        if !prefix.as_os_str().is_empty() {
            let name = prefix.to_string_lossy().replace('\\', "/");
            writer
                .add_directory(format!("{name}/"), stored_options)
                .map_err(|error| archive_error(destination, error))?;
        }
        append_zip_tree(
            &mut writer,
            source,
            prefix,
            destination,
            compressed_options,
            stored_options,
            exclude_python_artifacts,
        )?;
    }
    if let Some((name, content)) = extra_file {
        writer
            .start_file(name.replace('\\', "/"), compressed_options)
            .map_err(|error| archive_error(destination, error))?;
        writer
            .write_all(content)
            .map_err(|error| CoreError::io(destination, error))?;
    }
    writer
        .finish()
        .map_err(|error| archive_error(destination, error))?;
    Ok(())
}

fn append_zip_tree(
    writer: &mut ZipWriter<File>,
    source: &Path,
    prefix: &Path,
    destination: &Path,
    compressed_options: SimpleFileOptions,
    stored_options: SimpleFileOptions,
    exclude_python_artifacts: bool,
) -> Result<()> {
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
        if exclude_python_artifacts && is_python_development_artifact(entry.path()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| CoreError::InvalidInput("无法计算压缩包相对路径".into()))?;
        let name = prefix.join(relative).to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            writer
                .add_directory(format!("{name}/"), stored_options)
                .map_err(|error| archive_error(destination, error))?;
        } else if entry.file_type().is_file() {
            let size = entry
                .metadata()
                .map_err(|error| CoreError::io(entry.path(), io::Error::other(error)))?
                .len();
            let options = if should_store_entry(entry.path(), size) {
                stored_options
            } else {
                compressed_options
            };
            writer
                .start_file(&name, options)
                .map_err(|error| archive_error(destination, error))?;
            let mut input =
                File::open(entry.path()).map_err(|error| CoreError::io(entry.path(), error))?;
            io::copy(&mut input, &mut *writer)
                .map_err(|error| CoreError::io(destination, error))?;
        }
    }
    Ok(())
}

fn is_python_development_artifact(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("pyi") || extension.eq_ignore_ascii_case("pyc")
        })
}

fn should_store_entry(path: &Path, size: u64) -> bool {
    if size <= 4 * 1024 {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "7z" | "gif"
                | "gz"
                | "jpeg"
                | "jpg"
                | "mcaddon"
                | "mcpack"
                | "mp3"
                | "mp4"
                | "ogg"
                | "png"
                | "rar"
                | "webp"
                | "zip"
        )
    )
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

    #[test]
    fn rejects_a_corrupted_archive_without_leaving_output_files() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("broken.mcpack");
        fs::write(&archive_path, b"this is not a zip archive").unwrap();
        let destination = temp.path().join("extract");

        let error = extract_archive(&archive_path, &destination).unwrap_err();
        assert_eq!(error.code(), "archive_error");
        assert!(!destination.exists());
    }

    #[test]
    fn stores_small_and_precompressed_entries_but_deflates_large_text() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("small.json"), vec![b'a'; 4 * 1024]).unwrap();
        fs::write(source.join("large.json"), vec![b'a'; 4 * 1024 + 1]).unwrap();
        fs::write(source.join("texture.png"), vec![b'p'; 8 * 1024]).unwrap();
        let archive_path = temp.path().join("result.zip");

        write_zip(&source, &archive_path).unwrap();
        let mut archive = ZipArchive::new(File::open(archive_path).unwrap()).unwrap();
        assert_eq!(
            archive.by_name("small.json").unwrap().compression(),
            CompressionMethod::Stored
        );
        assert_eq!(
            archive.by_name("large.json").unwrap().compression(),
            CompressionMethod::Deflated
        );
        assert_eq!(
            archive.by_name("texture.png").unwrap().compression(),
            CompressionMethod::Stored
        );
    }

    #[test]
    fn addon_archives_exclude_python_stubs_and_bytecode_recursively() {
        let temp = tempfile::tempdir().unwrap();
        let behavior = temp.path().join("behavior_pack");
        fs::create_dir_all(behavior.join("scripts/cache")).unwrap();
        fs::write(behavior.join("scripts/main.py"), b"print('kept')").unwrap();
        fs::write(
            behavior.join("scripts/types.pyi"),
            b"def run() -> None: ...",
        )
        .unwrap();
        fs::write(behavior.join("scripts/cache/main.PYC"), b"bytecode").unwrap();
        let archive_path = temp.path().join("addon.zip");

        write_addon_zip_roots(&[behavior], &archive_path).unwrap();
        let mut archive = ZipArchive::new(File::open(archive_path).unwrap()).unwrap();
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_owned())
            .collect::<Vec<_>>();
        assert!(names.contains(&"behavior_pack/scripts/main.py".into()));
        assert!(!names.iter().any(|name| name.ends_with(".pyi")));
        assert!(
            !names
                .iter()
                .any(|name| name.to_ascii_lowercase().ends_with(".pyc"))
        );
    }
}
