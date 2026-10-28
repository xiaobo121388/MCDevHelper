use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{Cancellation, Result, error::invalid};
use sha2::{Digest, Sha256};

pub const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
pub const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 10_000;

pub(crate) fn relative_path(name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.len() > 1024 || name.starts_with('/') || name.contains('\\') {
        return Err(invalid(format!("Unsafe plugin path: {name}")));
    }
    for part in name.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(
                    &stem[3..],
                    "1" | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "\u{b9}"
                        | "\u{b2}"
                        | "\u{b3}"
                ))
        {
            return Err(invalid(format!("Unsafe plugin path: {name}")));
        }
    }
    Ok(PathBuf::from(name))
}

pub(crate) fn extract(source: &Path, destination: &Path) -> Result<()> {
    extract_cancelled(source, destination, &Cancellation::default())
}

pub(crate) fn extract_cancelled(
    source: &Path,
    destination: &Path,
    cancel: &Cancellation,
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(source)?).map_err(invalid)?;
    if zip.len() > MAX_ENTRIES {
        return Err(invalid("Too many archive entries"));
    }
    let mut names: BTreeMap<String, (String, bool)> = BTreeMap::new();
    let mut expanded = 0u64;
    fs::create_dir_all(destination)?;
    for index in 0..zip.len() {
        cancel.check()?;
        let mut entry = zip.by_index(index).map_err(invalid)?;
        let name = entry.name().trim_end_matches('/').to_owned();
        let relative = relative_path(&name)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o040000 | 0o100000))
        {
            return Err(invalid("Links and special archive entries are not allowed"));
        }
        let parts: Vec<_> = name.split('/').collect();
        for end in 1..=parts.len() {
            let prefix = parts[..end].join("/");
            let directory = end < parts.len() || entry.is_dir();
            if let Some((previous, was_directory)) = names.get(&prefix.to_lowercase()) {
                if previous != &prefix || !directory || !was_directory {
                    return Err(invalid(
                        "Duplicate, conflicting or case-colliding archive path",
                    ));
                }
            } else {
                names.insert(prefix.to_lowercase(), (prefix, directory));
            }
            if names.len() > MAX_ENTRIES {
                return Err(invalid("Too many archive files or implicit directories"));
            }
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(output)?;
            continue;
        }
        if entry.size() > MAX_EXPANDED - expanded {
            return Err(invalid("Expanded plugin exceeds 512 MiB"));
        }
        fs::create_dir_all(
            output
                .parent()
                .ok_or_else(|| invalid("Missing entry parent"))?,
        )?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        let copied = copy_stream(&mut entry, &mut file, MAX_EXPANDED - expanded, cancel)?;
        expanded += copied;
        if expanded > MAX_EXPANDED {
            return Err(invalid("Expanded plugin exceeds 512 MiB"));
        }
        file.flush()?;
    }
    Ok(())
}

pub(crate) fn regular_metadata(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err(invalid("Plugin links and special files are not allowed"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("Plugin reparse points are not allowed"));
        }
    }
    Ok(metadata)
}

fn copy_stream(
    input: &mut impl Read,
    output: &mut impl Write,
    limit: u64,
    cancel: &Cancellation,
) -> Result<u64> {
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        cancel.check()?;
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > limit {
            return Err(invalid("Plugin exceeds the import size limit"));
        }
        output.write_all(&buffer[..read])?;
    }
    Ok(size)
}

pub(crate) fn copy_bounded(
    source: &Path,
    destination: &Path,
    limit: u64,
    cancel: &Cancellation,
) -> Result<u64> {
    let metadata = regular_metadata(source)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid("Plugin exceeds the import size limit"));
    }
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    copy_stream(&mut input, &mut output, limit, cancel)
}

pub(crate) fn snapshot(source: &Path, destination: &Path, cancel: &Cancellation) -> Result<()> {
    let mut names = BTreeMap::new();
    let mut size = 0;
    let mut count = 0;
    fs::create_dir_all(destination)?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        cancel.check()?;
        let entry = entry.map_err(invalid)?;
        let metadata = regular_metadata(entry.path())?;
        if entry.path() == source {
            continue;
        }
        count += 1;
        if count > MAX_ENTRIES {
            return Err(invalid("Too many plugin entries"));
        }
        let name = entry
            .path()
            .strip_prefix(source)
            .map_err(invalid)?
            .to_str()
            .ok_or_else(|| invalid("Plugin paths must be valid UTF-8"))?
            .replace('\\', "/");
        let relative = relative_path(&name)?;
        if names.insert(name.to_lowercase(), ()).is_some() {
            return Err(invalid("Case-colliding plugin path"));
        }
        let target = destination.join(relative);
        if metadata.is_dir() {
            fs::create_dir(&target)?;
        } else {
            size += copy_bounded(entry.path(), &target, MAX_EXPANDED - size, cancel)?;
        }
    }
    Ok(())
}

pub(crate) fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let mut size = 0;
    let mut count = 0;
    let mut names = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(invalid)?;
        let metadata = regular_metadata(entry.path())?;
        if entry.path() == root {
            continue;
        }
        count += 1;
        if count > MAX_ENTRIES || entry.file_type().is_symlink() {
            return Err(invalid("Invalid plugin file tree"));
        }
        let name = entry
            .path()
            .strip_prefix(root)
            .map_err(invalid)?
            .to_str()
            .ok_or_else(|| invalid("Plugin paths must be valid UTF-8"))?
            .replace('\\', "/");
        relative_path(&name)?;
        if names.insert(name.to_lowercase(), ()).is_some() {
            return Err(invalid("Case-colliding plugin path"));
        }
        if metadata.is_file() {
            size += metadata.len();
            if size > MAX_EXPANDED {
                return Err(invalid("Expanded plugin exceeds 512 MiB"));
            }
            files.insert(name, hash_file(entry.path())?);
        }
    }
    Ok(files)
}

pub(crate) fn fingerprint(files: &BTreeMap<String, String>) -> String {
    let mut hash = Sha256::new();
    for (name, digest) in files {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(digest.as_bytes());
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}
