use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use mcdh_core::{ComponentService, DiscoveryService, ManagedProcess};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Cancellation, ExecutionResult, PluginCaller, PluginError, PluginService, Result, UiAction,
    error::invalid, store::InstalledPlugin,
};

const MAX_IO: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLog {
    pub time: String,
    pub level: String,
    pub text: String,
}

#[derive(Clone)]
pub(crate) struct Host {
    service: PluginService,
    record: InstalledPlugin,
    caller: PluginCaller,
    cancel: Cancellation,
    deadline: Instant,
    actions: Arc<Mutex<Vec<UiAction>>>,
}

impl Host {
    pub(crate) fn new(
        service: PluginService,
        record: InstalledPlugin,
        caller: PluginCaller,
        cancel: Cancellation,
        deadline: Instant,
        _: Option<String>,
    ) -> Result<Self> {
        let host = Self {
            service,
            record,
            caller,
            cancel,
            deadline,
            actions: Arc::default(),
        };
        fs::create_dir_all(host.data_directory())?;
        Ok(host)
    }

    pub(crate) fn data_directory(&self) -> PathBuf {
        self.service
            .root
            .join("data")
            .join(&self.record.manifest.id)
    }

    fn check(&self) -> Result<()> {
        self.cancel.check()?;
        if Instant::now() >= self.deadline {
            return Err(PluginError::new("timeout", "Plugin operation timed out"));
        }
        let current = self
            .service
            .registry()?
            .remove(&self.record.manifest.id)
            .ok_or_else(|| invalid("Plugin was uninstalled"))?;
        if !current.enabled
            || current.fingerprint != self.record.fingerprint
            || current.trusted_fingerprint.as_ref() != Some(&current.fingerprint)
            || (self.caller == PluginCaller::Mcp
                && current.mcp_fingerprint.as_ref() != Some(&current.fingerprint))
        {
            return Err(PluginError::new(
                "authorization_changed",
                "Plugin version or authorization changed",
            ));
        }
        Ok(())
    }

    pub(crate) fn actions(&self) -> Result<Vec<UiAction>> {
        Ok(self
            .actions
            .lock()
            .map_err(|_| invalid("UI action queue poisoned"))?
            .clone())
    }

    pub(crate) fn log(&self, level: &str, text: &str) -> Result<()> {
        let _guard = self.service.lease("registry", true)?;
        let path = self.data_directory().join("runtime.log");
        if path
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 1024 * 1024)
        {
            let old = path.with_extension("previous.log");
            if old.exists() {
                fs::remove_file(&old)?;
            }
            fs::rename(&path, old)?;
        }
        let entry = PluginLog {
            time: chrono::Utc::now().to_rfc3339(),
            level: level.chars().take(16).collect(),
            text: text.chars().take(8192).collect(),
        };
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        serde_json::to_writer(&mut file, &entry)?;
        file.write_all(b"\n")?;
        Ok(())
    }

    pub(crate) async fn call(&self, method: &str, args: Value) -> Result<Value> {
        self.check()?;
        let components = ComponentService::new(self.service.index.clone());
        match method {
            "components.list" => json_value(
                DiscoveryService::new(self.service.index.clone())
                    .refresh()?
                    .components,
            ),
            "components.get" => json_value(components.get_component(text(&args, "component_id")?)?),
            "components.create" => {
                json_value(components.create_component(&serde_json::from_value(args)?)?)
            }
            "components.import" => {
                json_value(components.import_component(&serde_json::from_value(args)?)?)
            }
            "components.copy" => {
                json_value(components.copy_component(&serde_json::from_value(args)?)?)
            }
            "components.move" => {
                json_value(components.move_component(&serde_json::from_value(args)?)?)
            }
            "components.export" => {
                json_value(components.export_component(&serde_json::from_value(args)?)?)
            }
            "components.metadata" => {
                json_value(components.set_component_metadata(&serde_json::from_value(args)?)?)
            }
            "components.uuids" => {
                json_value(components.regenerate_manifest_uuids(text(&args, "component_id")?)?)
            }
            "components.version" => {
                json_value(components.bump_manifest_version(&serde_json::from_value(args)?)?)
            }
            "fs.read_text" => {
                let path = absolute_path(&args)?;
                if fs::metadata(&path)?.len() > MAX_IO {
                    return Err(invalid("File exceeds 2 MiB text limit"));
                }
                Ok(Value::String(fs::read_to_string(path)?))
            }
            "fs.write_text" => {
                write_atomic(&absolute_path(&args)?, text(&args, "text")?.as_bytes())?;
                Ok(Value::Null)
            }
            "fs.read_directory" => {
                let mut entries = Vec::new();
                for entry in fs::read_dir(absolute_path(&args)?)? {
                    if entries.len() >= 10_000 {
                        return Err(invalid("Directory exceeds 10000 entries"));
                    }
                    let entry = entry?;
                    entries.push(json!({"name": entry.file_name().to_string_lossy(), "directory":entry.file_type()?.is_dir()}));
                }
                Ok(Value::Array(entries))
            }
            "fs.stat" => {
                let metadata = fs::metadata(absolute_path(&args)?)?;
                Ok(json!({"directory":metadata.is_dir(),"size":metadata.len()}))
            }
            "fs.mkdir" => {
                fs::create_dir_all(absolute_path(&args)?)?;
                Ok(Value::Null)
            }
            "fs.remove" => {
                let path = absolute_path(&args)?;
                if fs::symlink_metadata(&path)?.is_dir() {
                    fs::remove_dir(path)?;
                } else {
                    fs::remove_file(path)?;
                }
                Ok(Value::Null)
            }
            "http.request" => self.http(args).await,
            "process.run" => {
                let host = self.clone();
                tokio::task::spawn_blocking(move || host.process(args))
                    .await
                    .map_err(|e| PluginError::new("process_error", e))?
            }
            "settings.get" => Ok(self
                .service
                .registry()?
                .remove(&self.record.manifest.id)
                .ok_or_else(|| invalid("Plugin was uninstalled"))?
                .settings),
            "settings.set" => {
                let settings = args
                    .get("settings")
                    .ok_or_else(|| invalid("Missing settings"))?
                    .clone();
                crate::manifest::validate_value(&self.record.manifest.settings_schema, &settings)?;
                let _guard = self.service.lease("registry", true)?;
                let mut records = self.service.registry()?;
                records
                    .get_mut(&self.record.manifest.id)
                    .ok_or_else(|| invalid("Plugin was uninstalled"))?
                    .settings = settings.clone();
                self.service.save_registry(&records)?;
                Ok(settings)
            }
            "storage.get" | "storage.set" | "storage.delete" => self.storage(method, args),
            "log.write" => {
                self.log(text(&args, "level")?, text(&args, "text")?)?;
                Ok(Value::Null)
            }
            "ui.open_panel" => {
                if self.caller != PluginCaller::Desktop {
                    return Err(PluginError::new(
                        "ui_unavailable",
                        "UI is unavailable in the independent MCP host",
                    ));
                }
                let panel_id = text(&args, "panel_id")?;
                if !self
                    .record
                    .manifest
                    .panels
                    .iter()
                    .any(|panel| panel.id == panel_id)
                {
                    return Err(invalid("Unknown plugin panel"));
                }
                let placement = args["placement"].as_str().unwrap_or("main");
                if !matches!(placement, "main" | "window") {
                    return Err(invalid("Panel placement must be main or window"));
                }
                let mut actions = self
                    .actions
                    .lock()
                    .map_err(|_| invalid("UI action queue poisoned"))?;
                if actions.len() >= 8 {
                    return Err(invalid("Too many UI actions in one command"));
                }
                actions.push(UiAction {
                    panel_id: panel_id.into(),
                    placement: placement.into(),
                    component_id: args["component_id"].as_str().map(Into::into),
                });
                Ok(Value::Null)
            }
            _ => Err(PluginError::new(
                "unsupported_operation",
                format!("Unknown SDK operation: {method}"),
            )),
        }
    }

    fn storage(&self, method: &str, args: Value) -> Result<Value> {
        let key = text(&args, "key")?;
        if key.is_empty() || key.len() > 256 {
            return Err(invalid("Storage keys must be 1..256 bytes"));
        }
        let _guard = self.service.lease("registry", true)?;
        let path = self.data_directory().join("storage.json");
        let mut data: BTreeMap<String, Value> = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            BTreeMap::new()
        };
        match method {
            "storage.get" => return Ok(data.get(key).cloned().unwrap_or(Value::Null)),
            "storage.set" => {
                data.insert(
                    key.into(),
                    args.get("value").cloned().unwrap_or(Value::Null),
                );
            }
            "storage.delete" => {
                data.remove(key);
            }
            _ => return Err(invalid("Unknown storage operation")),
        }
        write_atomic(&path, &serde_json::to_vec(&data)?)?;
        Ok(Value::Null)
    }

    async fn http(&self, args: Value) -> Result<Value> {
        let url = reqwest::Url::parse(text(&args, "url")?).map_err(invalid)?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid(
                "HTTP SDK accepts http/https URLs without embedded credentials",
            ));
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let client = reqwest::Client::builder()
            .timeout(remaining)
            .connect_timeout(remaining.min(std::time::Duration::from_secs(15)))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("MCDH trusted plugin")
            .build()
            .map_err(|e| PluginError::new("network_error", e))?;
        let method =
            reqwest::Method::from_bytes(args["method"].as_str().unwrap_or("GET").as_bytes())
                .map_err(invalid)?;
        let mut request = client.request(method, url);
        if let Some(headers) = args["headers"].as_object() {
            for (name, value) in headers {
                request = request.header(
                    name,
                    value
                        .as_str()
                        .ok_or_else(|| invalid("HTTP headers must be strings"))?,
                );
            }
        }
        if let Some(body) = args["body"].as_str() {
            if body.len() as u64 > MAX_IO {
                return Err(invalid("HTTP body exceeds 2 MiB"));
            }
            request = request.body(body.to_owned());
        }
        let operation = async {
            let mut response = request
                .send()
                .await
                .map_err(|e| PluginError::new("network_error", e))?;
            let status = response.status().as_u16();
            let headers: BTreeMap<_, _> = response
                .headers()
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_owned()))
                .collect();
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| PluginError::new("network_error", e))?
            {
                if body.len() as u64 + chunk.len() as u64 > MAX_IO {
                    return Err(invalid("HTTP response exceeds 2 MiB"));
                }
                body.extend_from_slice(&chunk);
            }
            Ok(json!({"status":status,"headers":headers,"body":String::from_utf8_lossy(&body)}))
        };
        tokio::select! { result = operation => result, () = self.cancel.cancelled() => Err(PluginError::new("cancelled", "HTTP request cancelled")) }
    }

    fn process(&self, args: Value) -> Result<Value> {
        self.check()?;
        let program = PathBuf::from(text(&args, "program")?);
        if !program.is_absolute() || !program.is_file() {
            return Err(invalid("program must be an absolute executable file path"));
        }
        let program = mcdh_core::canonicalize_path(&program)?;
        let cwd = args["cwd"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or(self.data_directory());
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(invalid("cwd must be an existing absolute directory"));
        }
        let cwd = mcdh_core::canonicalize_path(&cwd)?;
        let arguments: Vec<String> =
            serde_json::from_value(args.get("args").cloned().unwrap_or_else(|| json!([])))?;
        let variables: BTreeMap<String, String> =
            serde_json::from_value(args.get("env").cloned().unwrap_or_else(|| json!({})))?;
        if variables
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
            || arguments.iter().any(|argument| argument.contains('\0'))
        {
            return Err(invalid("Invalid process arguments or environment"));
        }
        let mut process = ManagedProcess::start(
            &program,
            &arguments,
            &cwd,
            &variables.into_iter().collect::<Vec<_>>(),
        )?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| invalid("Missing process stdout"))?;
        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| invalid("Missing process stderr"))?;
        let out = std::thread::spawn(move || read_output(stdout));
        let err = std::thread::spawn(move || read_output(stderr));
        let deadline = args["timeout_ms"]
            .as_u64()
            .map(|ms| Instant::now() + std::time::Duration::from_millis(ms.min(600_000)))
            .unwrap_or(self.deadline)
            .min(self.deadline);
        let outcome = loop {
            if self.cancel.is_cancelled() {
                break Err(PluginError::new("cancelled", "Plugin process cancelled"));
            }
            if Instant::now() >= deadline {
                break Err(PluginError::new("timeout", "Plugin process timed out"));
            }
            match process.poll() {
                Ok(Some(code)) => break Ok(code),
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(error) => break Err(error.into()),
            }
        };
        drop(process);
        let (stdout, out_truncated) = out
            .join()
            .map_err(|_| invalid("Process stdout reader failed"))??;
        let (stderr, err_truncated) = err
            .join()
            .map_err(|_| invalid("Process stderr reader failed"))??;
        Ok(
            json!({"exit_code":outcome?,"stdout":stdout,"stderr":stderr,"truncated":out_truncated || err_truncated}),
        )
    }
}

impl PluginService {
    pub async fn panel_call(
        &self,
        id: &str,
        fingerprint: &str,
        method: &str,
        args: Value,
        cancel: Cancellation,
    ) -> Result<ExecutionResult> {
        let _lease = self.lease(id, false)?;
        let record = self.authorize(id, PluginCaller::Desktop)?;
        if record.fingerprint != fingerprint {
            return Err(invalid("Panel version changed; reopen the panel"));
        }
        let host = Host::new(
            self.clone(),
            record,
            PluginCaller::Desktop,
            cancel,
            Instant::now() + std::time::Duration::from_secs(60),
            None,
        )?;
        let result = host.call(method, args).await;
        if let Err(error) = &result {
            let _ = host.log("error", &error.to_string());
        }
        Ok(ExecutionResult {
            value: result?,
            actions: host.actions()?,
        })
    }

    pub fn logs(&self, id: &str) -> Result<Vec<PluginLog>> {
        if !crate::manifest::identifier(id) {
            return Err(invalid("Invalid plugin id"));
        }
        let path = self.root.join("data").join(id).join("runtime.log");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path)?;
        let mut logs: Vec<_> = text
            .lines()
            .rev()
            .take(200)
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        logs.reverse();
        Ok(logs)
    }
}

fn read_output(mut file: File) -> Result<(String, bool)> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let keep = read.min((512 * 1024usize).saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..keep]);
        truncated |= keep != read;
    }
    Ok((String::from_utf8_lossy(&output).into_owned(), truncated))
}

fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("Missing string: {name}")))
}
fn absolute_path(args: &Value) -> Result<PathBuf> {
    let path = PathBuf::from(text(args, "path")?);
    if !path.is_absolute() {
        return Err(invalid("Filesystem SDK requires absolute paths"));
    }
    Ok(path)
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_IO {
        return Err(invalid("Write exceeds 2 MiB"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(
        path.parent().ok_or_else(|| invalid("Path has no parent"))?,
    )?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| PluginError::from(error.error))?;
    Ok(())
}
fn json_value(value: impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(Into::into)
}
