use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use directories::BaseDirs;
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{CoreError, Result, SourceKind, SourceRecord};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('single', 'library')),
    path TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS component_metadata (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    tags_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
"#;

static PROCESS_LOCKS: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct LocalIndex {
    db_path: PathBuf,
    lock_path: PathBuf,
}

impl LocalIndex {
    pub fn open_default() -> Result<Self> {
        if let Some(directory) = std::env::var_os("MCDH_DATA_DIR") {
            return Self::open(PathBuf::from(directory).join("mcdh.db"));
        }
        let base = BaseDirs::new()
            .ok_or_else(|| CoreError::InvalidInput("无法定位本机应用数据目录".into()))?;
        Self::open(base.data_local_dir().join("MCDH").join("mcdh.db"))
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let db_path = path.into();
        let parent = db_path
            .parent()
            .ok_or_else(|| CoreError::InvalidInput("索引路径没有父目录".into()))?;
        fs::create_dir_all(parent).map_err(|error| CoreError::io(parent, error))?;
        let lock_path = parent.join("mutation.lock");
        let index = Self { db_path, lock_path };
        index.connection()?.execute_batch(SCHEMA)?;
        Ok(index)
    }

    pub fn path(&self) -> &Path {
        &self.db_path
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }

    pub fn try_lock_mutations(&self) -> Result<MutationGuard> {
        let process_locks = PROCESS_LOCKS.get_or_init(Mutex::default);
        {
            let mut locked_paths = process_locks
                .lock()
                .expect("mutation lock registry poisoned");
            if !locked_paths.insert(self.lock_path.clone()) {
                return Err(CoreError::Busy);
            }
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .map_err(|error| {
                release_process_lock(&self.lock_path);
                CoreError::io(&self.lock_path, error)
            })?;
        file.try_lock_exclusive().map_err(|error| {
            release_process_lock(&self.lock_path);
            if error.kind() == std::io::ErrorKind::WouldBlock {
                CoreError::Busy
            } else {
                CoreError::io(&self.lock_path, error)
            }
        })?;
        Ok(MutationGuard {
            file,
            lock_path: self.lock_path.clone(),
        })
    }

    pub fn add_source(&self, kind: SourceKind, path: impl AsRef<Path>) -> Result<SourceRecord> {
        if matches!(kind, SourceKind::McsAuto) {
            return Err(CoreError::InvalidInput(
                "MCS 自动来源不能写入本地来源表".into(),
            ));
        }
        let path = normalize_existing_path(path.as_ref())?;
        let path_text = path.to_string_lossy().into_owned();
        let kind_text = source_kind_text(kind);
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                "SELECT id, kind, path FROM sources WHERE path = ?1",
                [&path_text],
                source_from_row,
            )
            .optional()?;
        if let Some(source) = existing {
            return Ok(source);
        }
        let id = Uuid::new_v4().to_string();
        connection.execute(
            "INSERT INTO sources (id, kind, path) VALUES (?1, ?2, ?3)",
            params![id, kind_text, path_text],
        )?;
        Ok(SourceRecord { id, kind, path })
    }

    pub fn list_sources(&self) -> Result<Vec<SourceRecord>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id, kind, path FROM sources ORDER BY path COLLATE NOCASE")?;
        let sources = statement
            .query_map([], source_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sources)
    }

    pub fn remove_source(&self, id: &str) -> Result<bool> {
        let changed = self
            .connection()?
            .execute("DELETE FROM sources WHERE id = ?1", [id])?;
        Ok(changed > 0)
    }

    pub fn component_id(&self, path: impl AsRef<Path>) -> Result<String> {
        let path = normalize_path(path.as_ref())?;
        let path_text = path.to_string_lossy().into_owned();
        let connection = self.connection()?;
        if let Some(id) = connection
            .query_row(
                "SELECT id FROM component_metadata WHERE path = ?1",
                [&path_text],
                |row| row.get(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        connection.execute(
            "INSERT INTO component_metadata (id, path) VALUES (?1, ?2)",
            params![id, path_text],
        )?;
        Ok(id)
    }

    pub fn tags(&self, path: impl AsRef<Path>) -> Result<Vec<String>> {
        let path = normalize_path(path.as_ref())?;
        let path_text = path.to_string_lossy().into_owned();
        let connection = self.connection()?;
        let tags_json: Option<String> = connection
            .query_row(
                "SELECT tags_json FROM component_metadata WHERE path = ?1",
                [&path_text],
                |row| row.get(0),
            )
            .optional()?;
        tags_json
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| CoreError::json(&path, error))
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    pub fn set_tags(&self, path: impl AsRef<Path>, tags: &[String]) -> Result<String> {
        let path = normalize_path(path.as_ref())?;
        let id = self.component_id(&path)?;
        let tags = normalize_tags(tags);
        let tags_json = serde_json::to_string(&tags).expect("string lists always serialize");
        self.connection()?.execute(
            "UPDATE component_metadata SET tags_json = ?1 WHERE id = ?2",
            params![tags_json, id],
        )?;
        Ok(id)
    }

    pub fn move_component_metadata(
        &self,
        old_path: impl AsRef<Path>,
        new_path: impl AsRef<Path>,
    ) -> Result<bool> {
        let old_path = normalize_path(old_path.as_ref())?;
        let new_path = normalize_path(new_path.as_ref())?;
        let changed = self.connection()?.execute(
            "UPDATE component_metadata SET path = ?1 WHERE path = ?2",
            params![new_path.to_string_lossy(), old_path.to_string_lossy()],
        )?;
        Ok(changed > 0)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection()?
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.connection()?.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

pub struct MutationGuard {
    file: File,
    lock_path: PathBuf,
}

impl Drop for MutationGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
        release_process_lock(&self.lock_path);
    }
}

fn release_process_lock(path: &Path) {
    if let Some(locks) = PROCESS_LOCKS.get() {
        locks
            .lock()
            .expect("mutation lock registry poisoned")
            .remove(path);
    }
}

fn source_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    let kind: String = row.get(1)?;
    Ok(SourceRecord {
        id: row.get(0)?,
        kind: match kind.as_str() {
            "single" => SourceKind::Single,
            "library" => SourceKind::Library,
            _ => SourceKind::McsAuto,
        },
        path: PathBuf::from(row.get::<_, String>(2)?),
    })
}

fn source_kind_text(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::McsAuto => "mcs_auto",
        SourceKind::Single => "single",
        SourceKind::Library => "library",
    }
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        return Err(CoreError::NotFound(path.to_path_buf()));
    }
    fs::canonicalize(path).map_err(|error| CoreError::io(path, error))
}

fn normalize_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        normalize_existing_path(path)
    } else if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| CoreError::io(path, error))
    }
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized = tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    normalized.sort_by_key(|tag| tag.to_lowercase());
    normalized.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_sources_and_component_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let component = temp.path().join("组件一");
        fs::create_dir(&component).unwrap();
        let index = LocalIndex::open(temp.path().join("mcdh.db")).unwrap();

        let source = index.add_source(SourceKind::Single, &component).unwrap();
        assert_eq!(source.kind, SourceKind::Single);
        assert_eq!(index.list_sources().unwrap(), vec![source]);

        let first_id = index.component_id(&component).unwrap();
        let second_id = index.component_id(&component).unwrap();
        assert_eq!(first_id, second_id);

        index
            .set_tags(&component, &[" 开发 ".into(), "测试".into(), "开发".into()])
            .unwrap();
        assert_eq!(index.tags(&component).unwrap(), vec!["开发", "测试"]);
    }

    #[test]
    fn mutation_lock_is_exclusive() {
        let temp = tempfile::tempdir().unwrap();
        let first = LocalIndex::open(temp.path().join("mcdh.db")).unwrap();
        let second = LocalIndex::open(temp.path().join("mcdh.db")).unwrap();
        let guard = first.try_lock_mutations().unwrap();
        assert!(matches!(second.try_lock_mutations(), Err(CoreError::Busy)));
        drop(guard);
        second.try_lock_mutations().unwrap();
    }
}
