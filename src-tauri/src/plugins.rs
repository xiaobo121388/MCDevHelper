use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use mcdh_core::LocalIndex;
use mcdh_plugins::{
    Cancellation, ExecutionRequest, ExecutionResult, PluginCaller, PluginCandidate, PluginError,
    PluginLog, PluginPanelInfo, PluginProgress, PluginService, PluginSummary, Result,
};
use serde_json::Value;
use tauri::{Manager, State, ipc::Channel};

pub(crate) struct DesktopPlugins {
    pub service: PluginService,
    operations: Arc<Mutex<HashMap<String, Cancellation>>>,
}

struct Operation {
    id: String,
    operations: Arc<Mutex<HashMap<String, Cancellation>>>,
    cancellation: Cancellation,
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Ok(mut operations) = self.operations.lock() {
            operations.remove(&self.id);
        }
    }
}

impl DesktopPlugins {
    pub fn new(index: LocalIndex) -> Result<Self> {
        let service = PluginService::new(index)?;
        Ok(Self {
            service,
            operations: Arc::default(),
        })
    }
    fn operation(&self, id: String) -> Result<Operation> {
        if id.is_empty() || id.len() > 100 {
            return Err(PluginError::new(
                "invalid_operation",
                "Invalid plugin operation id",
            ));
        }
        let mut operations = self
            .operations
            .lock()
            .map_err(|_| PluginError::new("runtime_error", "Operation registry poisoned"))?;
        if operations.contains_key(&id) {
            return Err(PluginError::new(
                "plugin_busy",
                "Operation is already running",
            ));
        }
        let cancellation = Cancellation::default();
        operations.insert(id.clone(), cancellation.clone());
        Ok(Operation {
            id,
            operations: self.operations.clone(),
            cancellation,
        })
    }
    pub fn shutdown(&self) {
        if let Ok(operations) = self.operations.lock() {
            for cancellation in operations.values() {
                cancellation.cancel();
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn list_plugins(state: State<'_, DesktopPlugins>) -> Result<Vec<PluginSummary>> {
    let service = state.service.clone();
    blocking(move || service.list()).await
}

#[tauri::command]
pub(crate) async fn preview_plugin(
    state: State<'_, DesktopPlugins>,
    source: String,
    operation_id: String,
    on_progress: Channel<PluginProgress>,
) -> Result<PluginCandidate> {
    let operation = state.operation(operation_id)?;
    state
        .service
        .preview(&source, &operation.cancellation, move |progress| {
            let _ = on_progress.send(progress);
        })
        .await
}

#[tauri::command]
pub(crate) async fn preview_local_plugin(
    state: State<'_, DesktopPlugins>,
    path: String,
    operation_id: String,
) -> Result<PluginCandidate> {
    let operation = state.operation(operation_id)?;
    let service = state.service.clone();
    let cancellation = operation.cancellation.clone();
    blocking(move || service.preview_local(std::path::Path::new(&path), &cancellation)).await
}

#[tauri::command]
pub(crate) async fn install_plugin(
    state: State<'_, DesktopPlugins>,
    token: String,
    fingerprint: String,
) -> Result<PluginSummary> {
    let service = state.service.clone();
    blocking(move || service.install(&token, &fingerprint)).await
}

#[tauri::command]
pub(crate) fn discard_plugin_preview(
    state: State<'_, DesktopPlugins>,
    token: String,
) -> Result<()> {
    state.service.discard_preview(&token)
}

#[tauri::command]
pub(crate) async fn configure_plugin(
    state: State<'_, DesktopPlugins>,
    plugin_id: String,
    fingerprint: String,
    enabled: bool,
    allow_mcp: bool,
    settings: Value,
) -> Result<PluginSummary> {
    let service = state.service.clone();
    blocking(move || service.configure(&plugin_id, &fingerprint, enabled, allow_mcp, settings))
        .await
}

#[tauri::command]
pub(crate) async fn uninstall_plugin(
    state: State<'_, DesktopPlugins>,
    plugin_id: String,
    clear_data: bool,
) -> Result<()> {
    let service = state.service.clone();
    blocking(move || service.uninstall(&plugin_id, clear_data)).await
}

#[tauri::command]
pub(crate) async fn execute_plugin_command(
    state: State<'_, DesktopPlugins>,
    request: ExecutionRequest,
    operation_id: String,
) -> Result<ExecutionResult> {
    let operation = state.operation(operation_id)?;
    state
        .service
        .execute(
            request,
            PluginCaller::Desktop,
            operation.cancellation.clone(),
        )
        .await
}

#[tauri::command]
pub(crate) fn cancel_plugin_operation(
    state: State<'_, DesktopPlugins>,
    operation_id: String,
) -> Result<()> {
    if let Some(cancellation) = state
        .operations
        .lock()
        .map_err(|_| PluginError::new("runtime_error", "Operation registry poisoned"))?
        .get(&operation_id)
    {
        cancellation.cancel();
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_plugin_panel(
    state: State<'_, DesktopPlugins>,
    plugin_id: String,
    fingerprint: String,
    panel_id: String,
    component_id: Option<String>,
) -> Result<PluginPanelInfo> {
    let service = state.service.clone();
    blocking(move || service.panel_info(&plugin_id, &fingerprint, &panel_id, component_id)).await
}

#[tauri::command]
pub(crate) async fn plugin_panel_call(
    state: State<'_, DesktopPlugins>,
    plugin_id: String,
    fingerprint: String,
    method: String,
    args: Value,
    operation_id: String,
) -> Result<ExecutionResult> {
    let operation = state.operation(operation_id)?;
    state
        .service
        .panel_call(
            &plugin_id,
            &fingerprint,
            &method,
            args,
            operation.cancellation.clone(),
        )
        .await
}

#[tauri::command]
pub(crate) async fn get_plugin_logs(
    state: State<'_, DesktopPlugins>,
    plugin_id: String,
) -> Result<Vec<PluginLog>> {
    let service = state.service.clone();
    blocking(move || service.logs(&plugin_id)).await
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| PluginError::new("runtime_error", error))?
}

pub(crate) fn asset_response(
    service: &PluginService,
    path: &str,
    origin: &str,
) -> tauri::http::Response<Vec<u8>> {
    // Requests without Origin are images/styles loaded by an owned WebView, not a public HTTP server.
    let allowed = matches!(
        origin,
        "" | "http://tauri.localhost"
            | "https://tauri.localhost"
            | "tauri://localhost"
            | "http://localhost:1420"
    );
    let result = (|| {
        if !allowed {
            return Err(PluginError::new(
                "invalid_origin",
                "Plugin assets are only available to MCDH windows",
            ));
        }
        let decoded = percent_encoding::percent_decode_str(path.trim_start_matches('/'))
            .decode_utf8()
            .map_err(|e| PluginError::new("invalid_path", e))?;
        let mut parts = decoded.splitn(3, '/');
        let id = parts.next().unwrap_or("");
        let fingerprint = parts.next().unwrap_or("");
        let relative = parts.next().unwrap_or("");
        let bytes = service.asset(id, fingerprint, relative)?;
        let mime = match relative.rsplit('.').next().unwrap_or("") {
            "js" | "mjs" => "text/javascript; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "woff2" => "font/woff2",
            "woff" => "font/woff",
            _ => "application/octet-stream",
        };
        Ok((bytes, mime))
    })();
    let (status, bytes, mime) = match result {
        Ok((bytes, mime)) => (200, bytes, mime),
        Err(error) => (
            403,
            error.to_string().into_bytes(),
            "text/plain; charset=utf-8",
        ),
    };
    let mut builder = tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store");
    if allowed && !origin.is_empty() {
        builder = builder
            .header("Access-Control-Allow-Origin", origin)
            .header("Vary", "Origin");
    }
    builder.body(bytes).expect("static plugin response headers")
}

pub(crate) fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_uri_scheme_protocol("mcdh-plugin", |context, request| {
        asset_response(
            &context.app_handle().state::<DesktopPlugins>().service,
            request.uri().path(),
            request
                .headers()
                .get("Origin")
                .and_then(|value| value.to_str().ok())
                .unwrap_or(""),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn plugin_protocol_enforces_origin_inventory_version_and_enablement() {
        let temp = tempfile::tempdir().unwrap();
        let service =
            PluginService::new(LocalIndex::open(temp.path().join("mcdh.db")).unwrap()).unwrap();
        let archive = temp.path().join("fixture.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        for (name, bytes) in [("mcdh-plugin.json", json!({"schema_version":1,"api_version":1,"id":"example.fixture","name":"Fixture","version":"1.0.0","engines":{"mcdh":">=1.0.0"},"entry":"main.js"}).to_string()), ("main.js", "export function activate() {}".into()), ("icon.svg", "<svg/>".into())] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let preview = service.preview_archive("fixture/repo", &archive).unwrap();
        let plugin = service
            .install(&preview.token, &preview.fingerprint)
            .unwrap();
        let path = format!("/{}/{}/main.js", plugin.manifest.id, plugin.fingerprint);
        let response = asset_response(&service, &path, "http://tauri.localhost");
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()["Content-Type"],
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            response.headers()["Access-Control-Allow-Origin"],
            "http://tauri.localhost"
        );
        assert_eq!(
            asset_response(&service, &path, "https://unrelated.example").status(),
            403
        );
        assert_eq!(
            asset_response(&service, &path.replace("main.js", "icon.svg"), "").status(),
            200
        );
        assert_eq!(
            asset_response(
                &service,
                &path.replace("main.js", "../mcdh.db"),
                "http://tauri.localhost"
            )
            .status(),
            403
        );
        assert_eq!(
            asset_response(
                &service,
                &path.replace(&plugin.fingerprint, "stale"),
                "http://tauri.localhost"
            )
            .status(),
            403
        );
        service
            .configure(
                &plugin.manifest.id,
                &plugin.fingerprint,
                false,
                false,
                json!({}),
            )
            .unwrap();
        assert_eq!(
            asset_response(&service, &path, "http://tauri.localhost").status(),
            403
        );
    }
}
