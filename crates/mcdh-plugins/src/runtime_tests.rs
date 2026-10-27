use crate::{
    tests::{manifest, package, service},
    *,
};
use serde_json::{Value, json};
use std::{fs, time::Duration};

fn install(
    directory: &std::path::Path,
    definition: &Value,
    script: &str,
) -> (PluginService, PluginSummary) {
    let service = service(directory);
    let preview = service
        .preview_archive("owner/repo", &package(directory, definition, script))
        .unwrap();
    let plugin = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    (service, plugin)
}

fn request() -> ExecutionRequest {
    ExecutionRequest {
        plugin_id: "example.hello".into(),
        fingerprint: None,
        command: "hello".into(),
        input: json!({}),
        component_id: None,
        timeout_ms: 5_000,
    }
}

#[tokio::test]
async fn executes_async_sdk_io_settings_storage_logs_and_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let script = r#"
        let saved;
        export async function activate(ctx) {
            saved = ctx;
            console.log('activated');
            ctx.commands.register('hello', async () => {
                const path = ctx.environment.dataDir + '/message.txt';
                await ctx.fs.writeText(path, 'hello world');
                const text = await ctx.fs.readText(path);
                await ctx.storage.set('message', text);
                await ctx.settings.set({});
                return {text:await ctx.storage.get('message'), settings:await ctx.settings.get()};
            });
        }
        export async function deactivate() { await saved.storage.set('deactivated', true); }
    "#;
    let (service, plugin) = install(temp.path(), &manifest(), script);
    let result = service
        .execute(request(), PluginCaller::Desktop, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(result.value, json!({"text":"hello world","settings":{}}));
    assert_eq!(
        service.logs(&plugin.manifest.id).unwrap()[0].text,
        "activated"
    );
    let storage: Value = serde_json::from_slice(
        &fs::read(service.root.join("data/example.hello/storage.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(storage["deactivated"], true);
}

#[tokio::test]
async fn mcp_authorization_and_ui_boundaries_are_checked_on_every_call() {
    let temp = tempfile::tempdir().unwrap();
    let (service, plugin) = install(
        temp.path(),
        &manifest(),
        "export function activate(ctx) { ctx.commands.register('hello', () => ctx.ui.openPanel('main')); }",
    );
    assert_eq!(
        service
            .execute(request(), PluginCaller::Mcp, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "mcp_not_authorized"
    );
    service
        .configure(
            &plugin.manifest.id,
            &plugin.fingerprint,
            true,
            true,
            json!({}),
        )
        .unwrap();
    assert_eq!(
        service
            .execute(request(), PluginCaller::Mcp, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    let result = service
        .execute(request(), PluginCaller::Desktop, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(result.actions[0].panel_id, "main");
    service
        .configure(
            &plugin.manifest.id,
            &plugin.fingerprint,
            true,
            false,
            json!({}),
        )
        .unwrap();
    assert_eq!(
        service
            .execute(request(), PluginCaller::Mcp, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "mcp_not_authorized"
    );
}

#[tokio::test]
async fn validates_inputs_and_outputs_without_running_invalid_calls() {
    let temp = tempfile::tempdir().unwrap();
    let mut definition = manifest();
    definition["commands"][0]["input_schema"] = json!({"type":"object","required":["name"],"properties":{"name":{"type":"string"}},"additionalProperties":false});
    definition["commands"][0]["output_schema"] = json!({"type":"integer"});
    let (service, _) = install(
        temp.path(),
        &definition,
        "export function activate(ctx) { ctx.commands.register('hello', () => 'wrong output'); }",
    );
    assert_eq!(
        service
            .execute(request(), PluginCaller::Desktop, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "invalid_arguments"
    );
    let mut call = request();
    call.input = json!({"name":"valid"});
    assert_eq!(
        service
            .execute(call, PluginCaller::Desktop, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "invalid_arguments"
    );
}

#[tokio::test]
async fn interrupts_cpu_loops_unsettled_promises_and_explicit_cancellation() {
    for body in ["while(true) {}", "await new Promise(() => {})"] {
        let temp = tempfile::tempdir().unwrap();
        let (service, _) = install(
            temp.path(),
            &manifest(),
            &format!(
                "export function activate(ctx) {{ ctx.commands.register('hello', async () => {{ {body} }}); }}"
            ),
        );
        let mut call = request();
        call.timeout_ms = 100;
        let begin = std::time::Instant::now();
        assert_eq!(
            service
                .execute(call, PluginCaller::Desktop, Cancellation::default())
                .await
                .unwrap_err()
                .code,
            "timeout"
        );
        assert!(begin.elapsed() < Duration::from_secs(3));
        let cancel = Cancellation::default();
        let token = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            token.cancel();
        });
        assert_eq!(
            service
                .execute(request(), PluginCaller::Desktop, cancel)
                .await
                .unwrap_err()
                .code,
            "cancelled"
        );
    }
}

#[tokio::test]
async fn tampering_revokes_trust_and_does_not_execute_modified_code() {
    let temp = tempfile::tempdir().unwrap();
    let (service, plugin) = install(
        temp.path(),
        &manifest(),
        "export function activate(ctx) { ctx.commands.register('hello', () => 1); }",
    );
    service
        .configure(
            &plugin.manifest.id,
            &plugin.fingerprint,
            true,
            true,
            json!({}),
        )
        .unwrap();
    let record = service.registry().unwrap().remove("example.hello").unwrap();
    fs::write(
        service.version_directory(&record).unwrap().join("main.js"),
        "throw new Error('altered');",
    )
    .unwrap();
    assert_eq!(
        service
            .execute(request(), PluginCaller::Mcp, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "integrity_error"
    );
    let current = service.list().unwrap().remove(0);
    assert!(!current.enabled && !current.allow_mcp && !current.trusted);
}

#[tokio::test]
async fn component_sdk_uses_the_existing_core_service_and_version_is_pinned() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let (service, plugin) = install(
        temp.path(),
        &manifest(),
        "export function activate(ctx) { ctx.commands.register('hello', () => ctx.components.get(ctx.environment.componentId)); }",
    );
    service
        .index
        .add_source(mcdh_core::SourceKind::Single, &project)
        .unwrap();
    let id = service.index.component_id(&project).unwrap();
    fs::write(project.join("level.dat"), []).unwrap();
    let mut call = request();
    call.component_id = Some(id.clone());
    call.fingerprint = Some("stale".into());
    assert_eq!(
        service
            .execute(call.clone(), PluginCaller::Desktop, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "plugin_version_changed"
    );
    call.fingerprint = Some(plugin.fingerprint);
    let result = service
        .execute(call, PluginCaller::Desktop, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(result.value["id"], id);
    assert_eq!(result.value["kind"], "map");
}

#[tokio::test]
async fn http_sdk_uses_real_local_requests_and_bounds_failure() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/fixture", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let read = socket.read(&mut request).unwrap();
        assert!(read > 0);
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello")
            .unwrap();
    });
    let temp = tempfile::tempdir().unwrap();
    let script = format!(
        "export function activate(ctx) {{ ctx.commands.register('hello', async () => (await ctx.http.request({{url:{}}})).body); }}",
        serde_json::to_string(&url).unwrap()
    );
    let (service, _) = install(temp.path(), &manifest(), &script);
    assert_eq!(
        service
            .execute(request(), PluginCaller::Desktop, Cancellation::default())
            .await
            .unwrap()
            .value,
        "hello"
    );
    server.join().unwrap();
}

#[tokio::test]
#[cfg(windows)]
async fn process_sdk_preserves_literal_arguments_and_cleans_up_on_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let program =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
    let script = format!(
        r#"export function activate(ctx) {{ ctx.commands.register('hello', () => ctx.process.run({{program:{}, args:['/d','/c','echo plugin-output']}})); }}"#,
        serde_json::to_string(&program).unwrap()
    );
    let (service, _) = install(temp.path(), &manifest(), &script);
    let value = service
        .execute(request(), PluginCaller::Desktop, Cancellation::default())
        .await
        .unwrap()
        .value;
    assert_eq!(value["exit_code"], 0, "{value}");
    assert!(value["stdout"].as_str().unwrap().contains("plugin-output"));
    let script = format!(
        r#"export function activate(ctx) {{ ctx.commands.register('hello', () => ctx.process.run({{program:{}, args:['/d','/c','ping -n 30 127.0.0.1 > nul'], timeout_ms:100}})); }}"#,
        serde_json::to_string(&program).unwrap()
    );
    let preview = service
        .preview_archive("owner/repo", &package(temp.path(), &manifest(), &script))
        .unwrap();
    service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    let begin = std::time::Instant::now();
    assert_eq!(
        service
            .execute(request(), PluginCaller::Desktop, Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "timeout"
    );
    assert!(begin.elapsed() < Duration::from_secs(3));
    service.uninstall("example.hello", true).unwrap();
}
