use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{Result, error::invalid};
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
    let mut zip = zip::ZipArchive::new(File::open(source)?).map_err(invalid)?;
    if zip.len() > MAX_ENTRIES {
        return Err(invalid("Too many archive entries"));
    }
    let mut names: BTreeMap<String, (String, bool)> = BTreeMap::new();
    let mut expanded = 0u64;
    fs::create_dir_all(destination)?;
    for index in 0..zip.len() {
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
        let copied = std::io::copy(
            &mut (&mut entry).take(MAX_EXPANDED - expanded + 1),
            &mut file,
        )?;
        expanded += copied;
        if expanded > MAX_EXPANDED {
            return Err(invalid("Expanded plugin exceeds 512 MiB"));
        }
        file.flush()?;
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
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(invalid)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("Plugin links are not allowed"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("Plugin reparse points are not allowed"));
            }
        }
        if entry.path() == root {
            continue;
        }
        count += 1;
        if count > MAX_ENTRIES || entry.file_type().is_symlink() {
            return Err(invalid("Invalid plugin file tree"));
        }
        if entry.file_type().is_file() {
            size += entry.metadata().map_err(invalid)?.len();
            if size > MAX_EXPANDED {
                return Err(invalid("Expanded plugin exceeds 512 MiB"));
            }
            let name = entry
                .path()
                .strip_prefix(root)
                .map_err(invalid)?
                .to_string_lossy()
                .replace('\\', "/");
            relative_path(&name)?;
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
