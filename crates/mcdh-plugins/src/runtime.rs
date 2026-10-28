use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use rquickjs::{
    AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Module,
    loader::{ImportAttributes, Loader, Resolver},
    module::Declared,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, oneshot};

use crate::{Cancellation, PluginError, PluginService, Result, archive, error::invalid, sdk::Host};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCaller {
    Desktop,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRequest {
    pub plugin_id: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
    pub command: String,
    #[serde(default = "empty_object")]
    pub input: Value,
    #[serde(default)]
    pub component_id: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}
fn empty_object() -> Value {
    json!({})
}
fn default_timeout() -> u64 {
    60_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiAction {
    pub panel_id: String,
    pub placement: String,
    pub component_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionResult {
    pub value: Value,
    pub actions: Vec<UiAction>,
}

pub(crate) struct CancelOnDrop(pub(crate) Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl PluginService {
    pub async fn execute(
        &self,
        request: ExecutionRequest,
        caller: PluginCaller,
        cancellation: Cancellation,
    ) -> Result<ExecutionResult> {
        if request.plugin_id == crate::MCDE_PLUGIN_ID {
            let _lease = self.lease(crate::MCDE_PLUGIN_ID, false)?;
            self.authorize(crate::MCDE_PLUGIN_ID, caller)?;
            return Err(PluginError::new(
                "ui_unavailable",
                "Native MCDE commands require the desktop application",
            ));
        }
        if request.timeout_ms == 0 || request.timeout_ms > 600_000 {
            return Err(invalid("timeout_ms must be between 1 and 600000"));
        }
        if serde_json::to_vec(&request.input)?.len() > 128 * 1024 {
            return Err(invalid("Plugin arguments exceed 128 KiB"));
        }
        static WORKERS: OnceLock<Arc<Semaphore>> = OnceLock::new();
        let _cancel_on_drop = CancelOnDrop(cancellation.clone());
        let acquire = WORKERS
            .get_or_init(|| Arc::new(Semaphore::new(16)))
            .clone()
            .acquire_owned();
        let permit = tokio::select! {
            permit = acquire => permit.map_err(invalid)?,
            () = cancellation.cancelled() => return Err(PluginError::new("cancelled", "Plugin command cancelled while queued")),
        };
        let (sender, receiver) = oneshot::channel();
        let service = self.clone();
        std::thread::Builder::new()
            .name(format!("plugin-{}", request.plugin_id))
            .spawn(move || {
                let _permit = permit;
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| PluginError::new("runtime_error", e))
                    .and_then(|runtime| {
                        runtime.block_on(service.run(request.clone(), caller, cancellation))
                    });
                if let Err(error) = &result {
                    let _ = service.record_error(&request.plugin_id, error);
                }
                let _ = sender.send(result);
            })?;
        receiver.await.map_err(|_| {
            PluginError::new("runtime_error", "Plugin worker terminated unexpectedly")
        })?
    }

    async fn run(
        &self,
        request: ExecutionRequest,
        caller: PluginCaller,
        cancellation: Cancellation,
    ) -> Result<ExecutionResult> {
        let _cancel_on_drop = CancelOnDrop(cancellation.clone());
        cancellation.check()?;
        let _lease = self.lease(&request.plugin_id, false)?;
        let record = self.authorize(&request.plugin_id, caller)?;
        if request
            .fingerprint
            .as_ref()
            .is_some_and(|fingerprint| fingerprint != &record.fingerprint)
        {
            return Err(PluginError::new(
                "plugin_version_changed",
                "Plugin version changed; reopen its panel or command",
            ));
        }
        let command = record
            .manifest
            .commands
            .iter()
            .find(|command| command.id == request.command)
            .ok_or_else(|| invalid("Unknown plugin command"))?;
        crate::manifest::validate_value(&command.input_schema, &request.input)?;
        let deadline = Instant::now() + Duration::from_millis(request.timeout_ms);
        let host = Host::new(
            self.clone(),
            record.clone(),
            caller,
            cancellation.clone(),
            deadline,
            request.component_id.clone(),
        )?;
        let runtime = AsyncRuntime::new().map_err(engine_error)?;
        runtime.set_memory_limit(64 * 1024 * 1024).await;
        let interrupted = cancellation.clone();
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                interrupted.is_cancelled() || Instant::now() >= deadline
            })))
            .await;
        let root = self.version_directory(&record)?;
        runtime
            .set_loader(
                PackageResolver(record.files.clone()),
                PackageLoader {
                    root,
                    files: record.files.clone(),
                },
            )
            .await;
        let context = AsyncContext::full(&runtime).await.map_err(engine_error)?;
        let metadata = json!({"commands": record.manifest.commands, "environment":{
            "caller": caller, "pluginId": record.manifest.id, "appVersion": mcdh_core::VERSION,
            "packageDir": self.version_directory(&record)?, "dataDir": host.data_directory(), "componentId":request.component_id
        }});
        let source = format!(
            r#"
            import {{ createContext }} from 'mcdh:sdk';
            const state = createContext(async (method, args) => JSON.parse(await __mcdh_native(method, JSON.stringify(args))), {metadata}, __mcdh_log);
            globalThis.console = {{ log: state.context.log.info, info: state.context.log.info, warn: state.context.log.warn, error: state.context.log.error }};
            const entry = await import({entry});
            let result;
            try {{
                if (typeof entry.activate !== 'function') throw new Error('Backend must export activate(ctx)');
                await entry.activate(state.context);
                const handler = state.handlers.get({command});
                if (!handler) throw new Error('Plugin did not register the declared command handler');
                result = JSON.stringify({{ok:true, value: (await handler({input}, state.context)) ?? null}});
            }} catch (error) {{
                result = JSON.stringify({{ok:false, error:{{ code:String(error?.code || 'plugin_error'), message:String(error?.message || error) + (error?.stack ? '\n' + error.stack : '') }} }});
            }} finally {{
                try {{ if (typeof entry.deactivate === 'function') await entry.deactivate(); }}
                finally {{ state.dispose(); }}
            }}
            export const output = result;
        "#,
            entry = serde_json::to_string(&record.manifest.entry)?,
            command = serde_json::to_string(&request.command)?,
            input = serde_json::to_string(&request.input)?,
            metadata = metadata
        );
        let operation = context.async_with(async |ctx| -> Result<Value> {
            let bridge = host.clone();
            ctx.globals().set("__mcdh_native", rquickjs::Function::new(ctx.clone(), rquickjs::function::Async(move |method: String, args: String| {
                let host = bridge.clone();
                async move { serde_json::to_string(&match serde_json::from_str::<Value>(&args) {
                    Ok(args) => match host.call(&method, args).await { Ok(value) => json!({"ok":true,"value":value}), Err(error) => json!({"ok":false,"error":error}) },
                    Err(error) => json!({"ok":false,"error":PluginError::from(error)}),
                }).unwrap_or_else(|_| "{\"ok\":false,\"error\":{\"code\":\"serialization_error\",\"message\":\"Cannot serialize host response\"}}".into()) }
            })).map_err(engine_error)?).map_err(engine_error)?;
            let logger = host.clone();
            ctx.globals().set("__mcdh_log", rquickjs::Function::new(ctx.clone(), move |level: String, text: String| { let _ = logger.log(&level, &text); }).map_err(engine_error)?).map_err(engine_error)?;
            let module = Module::declare(ctx.clone(), "__mcdh_runner__.js", source).catch(&ctx).map_err(|e| PluginError::new("plugin_error", e))?;
            let (module, promise) = module.eval().catch(&ctx).map_err(|e| PluginError::new("plugin_error", e))?;
            promise.into_future::<()>().await.catch(&ctx).map_err(|e| PluginError::new("plugin_error", e))?;
            let output: String = module.get("output").catch(&ctx).map_err(|e| PluginError::new("plugin_error", e))?;
            if output.len() > 2 * 1024 * 1024 { return Err(invalid("Plugin result exceeds 2 MiB")); }
            let envelope: Value = serde_json::from_str(&output)?;
            if envelope["ok"] != true { return Err(serde_json::from_value(envelope["error"].clone())?); }
            Ok(envelope["value"].clone())
        });
        let result = tokio::select! {
            result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), operation) => match result {
                Ok(result) => result,
                Err(_) => Err(PluginError::new("timeout", "Plugin command timed out")),
            },
            () = cancellation.cancelled() => Err(PluginError::new("cancelled", "Plugin command cancelled"))
        };
        if cancellation.is_cancelled() {
            return Err(PluginError::new("cancelled", "Plugin command cancelled"));
        }
        if Instant::now() >= deadline {
            return Err(PluginError::new("timeout", "Plugin command timed out"));
        }
        let value = result?;
        if let Some(schema) = &command.output_schema {
            crate::manifest::validate_value(schema, &value)?;
        }
        Ok(ExecutionResult {
            value,
            actions: host.actions()?,
        })
    }

    pub(crate) fn authorize(
        &self,
        id: &str,
        caller: PluginCaller,
    ) -> Result<crate::store::InstalledPlugin> {
        let record = self
            .registry()?
            .remove(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        if !record.enabled {
            return Err(PluginError::new("plugin_disabled", "Plugin is disabled"));
        }
        if caller == PluginCaller::Mcp
            && record.mcp_fingerprint.as_ref() != Some(&record.fingerprint)
        {
            return Err(PluginError::new(
                "mcp_not_authorized",
                "Authorize this exact plugin version in the desktop application first",
            ));
        }
        self.verify(&record).inspect_err(|error| {
            let _ = self.record_error(id, error);
        })?;
        Ok(record)
    }

    pub(crate) fn record_error(&self, id: &str, error: &PluginError) -> Result<()> {
        let _registry = self.lease("registry", true)?;
        let mut records = self.registry()?;
        if let Some(record) = records.get_mut(id) {
            record.last_error = Some(error.clone());
            if error.code == "integrity_error" {
                record.trusted_fingerprint = None;
                record.mcp_fingerprint = None;
                record.enabled = false;
            }
            self.save_registry(&records)?;
        }
        Ok(())
    }
}

fn engine_error(error: rquickjs::Error) -> PluginError {
    PluginError::new("runtime_error", error)
}

struct PackageResolver(BTreeMap<String, String>);
impl Resolver for PackageResolver {
    fn resolve<'js>(
        &mut self,
        _: &Ctx<'js>,
        base: &str,
        name: &str,
        _: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if name == "mcdh:sdk" {
            return Ok(name.into());
        }
        let mut parts: Vec<&str> = if name.starts_with('.') {
            base.rsplit_once('/')
                .map(|(parent, _)| parent.split('/').collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for part in name.split('/') {
            match part {
                "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err(rquickjs::Error::new_resolving(base, name));
                    }
                }
                part => parts.push(part),
            }
        }
        let resolved = parts.join("/");
        if archive::relative_path(&resolved).is_err() || !self.0.contains_key(&resolved) {
            return Err(rquickjs::Error::new_resolving(base, name));
        }
        Ok(resolved)
    }
}

struct PackageLoader {
    root: PathBuf,
    files: BTreeMap<String, String>,
}
impl Loader for PackageLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if name == "mcdh:sdk" {
            return Module::declare(ctx.clone(), name, include_str!("../../../sdk/context.js"));
        }
        let path = archive::relative_path(name).map_err(|_| rquickjs::Error::new_loading(name))?;
        let path = self.root.join(path);
        let hash = archive::hash_file(&path).map_err(|_| rquickjs::Error::new_loading(name))?;
        if self.files.get(name) != Some(&hash) {
            return Err(rquickjs::Error::new_loading(name));
        }
        Module::declare(
            ctx.clone(),
            name,
            std::fs::read(path).map_err(|_| rquickjs::Error::new_loading(name))?,
        )
    }
}
