use mcdh_core::LocalIndex;
use mcdh_plugins::PluginService;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
use zip::{ZipWriter, write::SimpleFileOptions};

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    id: u64,
    notifications: Vec<Value>,
}
impl Client {
    fn start(data: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mcdh-mcp"))
            .env("MCDH_DATA_DIR", data)
            .env("MCDH_DISABLE_MCS_SCAN", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut client = Self {
            input: child.stdin.take(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            id: 0,
            notifications: Vec::new(),
        };
        client.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"plugin-contract","version":"1"}}));
        client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        client
    }
    fn send(&mut self, value: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }
    fn begin(&mut self, method: &str, params: Value) -> u64 {
        self.id += 1;
        let id = self.id;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        id
    }
    fn receive(&mut self, id: u64) -> Value {
        loop {
            let mut line = String::new();
            self.output.read_line(&mut line).unwrap();
            assert!(!line.is_empty(), "MCP closed unexpectedly");
            let response: Value =
                serde_json::from_str(&line).expect("plugin logs must never enter stdout");
            if response.get("id").is_none() {
                assert!(response.get("method").is_some(), "{response}");
                self.notifications.push(response);
                continue;
            }
            assert_eq!(response["id"], id, "{response}");
            return response;
        }
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.begin(method, params);
        self.receive(id)
    }
    fn tools(&mut self) -> Vec<String> {
        self.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":arguments}))
    }
    fn finish(mut self) {
        drop(self.input.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "MCP did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(stderr.contains("started on stdio"));
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn package(directory: &Path, version: &str) -> std::path::PathBuf {
    let path = directory.join(format!("{version}.zip"));
    let definition = json!({"schema_version":1,"api_version":1,"id":"example.contract","name":"Contract","version":version,"engines":{"mcdh":">=1.0.0"},"entry":"main.js",
        "commands":[
            {"id":"echo","title":"Echo","input_schema":{"type":"object","properties":{"value":{"type":"integer"}},"required":["value"],"additionalProperties":false},"output_schema":{"type":"object","properties":{"echo":{"type":"integer"}},"required":["echo"]}},
            {"id":"ui","title":"UI"}, {"id":"hold","title":"Hold","input_schema":{"type":"object","properties":{"delay":{"type":"integer","minimum":100,"maximum":10000}},"additionalProperties":false}}
        ],
        "panels":[{"id":"main","title":"Panel","entry":"panel.js"}],
        "mcp_tools":[{"name":"echo","command":"echo","description":"Echo integer"},{"name":"ui","command":"ui","description":"UI boundary"},{"name":"hold","command":"hold","description":"Hold a process lease"}]});
    let system = std::env::var("SystemRoot")
        .unwrap_or_else(|_| "C:/Windows".into())
        .replace('\\', "/");
    let script = format!(
        r#"
        export function activate(ctx) {{
            console.log('plugin-activated');
            ctx.commands.register('echo', async (input) => {{ await ctx.fs.writeText(ctx.environment.dataDir + '/executed.txt','yes'); return {{echo:input.value}}; }});
            ctx.commands.register('ui', () => ctx.ui.openPanel('main'));
            ctx.commands.register('hold', async (input) => {{
                await ctx.fs.writeText(ctx.environment.dataDir + '/holding.txt','yes');
                return ctx.process.run({{program:{},args:['-NoProfile','-NonInteractive','-Command','Start-Sleep -Milliseconds ' + (input.delay || 500)]}});
            }});
        }}
    "#,
        serde_json::to_string(&format!(
            "{system}/System32/WindowsPowerShell/v1.0/powershell.exe"
        ))
        .unwrap()
    );
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, bytes) in [
        ("mcdh-plugin.json", definition.to_string()),
        ("main.js", script),
        ("panel.js", "export function mount() {}".into()),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
#[cfg(windows)]
fn plugins_are_independent_version_bound_and_never_pollute_stdio() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("state");
    let service = PluginService::new(LocalIndex::open(data.join("mcdh.db")).unwrap()).unwrap();
    let first = service
        .preview_archive("fixture/plugin", &package(temp.path(), "1.0.0"))
        .unwrap();
    let plugin = service.install(&first.token, &first.fingerprint).unwrap();
    let mut client = Client::start(&data);
    assert_eq!(client.tools().len(), 27);
    assert!(
        !data
            .join("plugins/data/example.contract/executed.txt")
            .exists()
    );
    assert_eq!(
        client.call("plugin__example.contract__echo", json!({"value":7}))["result"]["isError"],
        true
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
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(client.tools().len(), 30);
    assert!(
        client
            .notifications
            .iter()
            .any(|notification| notification["method"] == "notifications/tools/list_changed")
    );
    assert!(
        !data
            .join("plugins/data/example.contract/executed.txt")
            .exists(),
        "tool discovery must not activate the plugin"
    );
    assert_eq!(
        client.call("plugin__example.contract__echo", json!({"value":"invalid"}))["result"]["isError"],
        true
    );
    assert!(
        !data
            .join("plugins/data/example.contract/executed.txt")
            .exists()
    );
    let result = client.call("plugin__example.contract__echo", json!({"value":7}));
    assert_eq!(
        result["result"]["structuredContent"],
        json!({"result":{"echo":7}}),
        "{result}"
    );
    assert!(
        data.join("plugins/data/example.contract/executed.txt")
            .is_file()
    );
    let ui = client.call("plugin__example.contract__ui", json!({}));
    assert_eq!(ui["result"]["isError"], true);
    assert!(ui.to_string().contains("ui_unavailable"));
    let pending = client.begin(
        "tools/call",
        json!({"name":"plugin__example.contract__hold","arguments":{}}),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !data
        .join("plugins/data/example.contract/holding.txt")
        .exists()
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        service
            .uninstall(&plugin.manifest.id, false)
            .unwrap_err()
            .code,
        "plugin_busy"
    );
    assert_eq!(client.receive(pending)["result"]["isError"], false);
    let marker = data.join("plugins/data/example.contract/holding.txt");
    fs::remove_file(&marker).unwrap();
    let pending = client.begin(
        "tools/call",
        json!({"name":"plugin__example.contract__hold","arguments":{"delay":5000}}),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    client.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":pending,"reason":"contract cancellation"}}));
    // MCP drops responses for cancelled IDs. A subsequent request must still work.
    assert_eq!(client.tools().len(), 30);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match service.configure(
            &plugin.manifest.id,
            &plugin.fingerprint,
            true,
            false,
            json!({}),
        ) {
            Ok(_) => break,
            Err(error) => {
                assert_eq!(error.code, "plugin_busy");
                assert!(
                    Instant::now() < deadline,
                    "cancelled process did not release its lease"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    assert_eq!(client.tools().len(), 27);
    assert_eq!(
        client.call("plugin__example.contract__echo", json!({"value":1}))["result"]["isError"],
        true
    );
    let second = service
        .preview_archive("fixture/plugin", &package(temp.path(), "1.0.1"))
        .unwrap();
    let installed = service.install(&second.token, &second.fingerprint).unwrap();
    assert!(!installed.allow_mcp);
    service
        .configure(
            &installed.manifest.id,
            &installed.fingerprint,
            true,
            true,
            json!({}),
        )
        .unwrap();
    let source = data
        .join("plugins/installed/example.contract")
        .join(&installed.fingerprint)
        .join("main.js");
    fs::write(source, "throw new Error('modified');").unwrap();
    let failed = client.call("plugin__example.contract__echo", json!({"value":1}));
    assert!(failed.to_string().contains("integrity_error"), "{failed}");
    assert_eq!(client.tools().len(), 27);
    assert!(!service.list().unwrap()[0].allow_mcp);
    client.finish();
}
