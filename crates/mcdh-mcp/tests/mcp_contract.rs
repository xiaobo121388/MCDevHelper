use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn start(data_directory: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mcdh-mcp"))
            .env("MCDH_DATA_DIR", data_directory)
            .env("MCDH_DISABLE_MCS_SCAN", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start mcdh-mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }));
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        assert!(
            !line.is_empty(),
            "server closed before responding to {method}"
        );
        let response: Value = serde_json::from_str(&line).expect("JSON-RPC response");
        assert_eq!(response["id"], id, "unexpected response for {method}");
        response
    }

    fn notify(&mut self, method: &str) {
        self.send(json!({"jsonrpc": "2.0", "method": method}));
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        assert_eq!(
            response["result"]["isError"], false,
            "{name} failed: {response}"
        );
        response["result"]["structuredContent"].clone()
    }

    fn call_error(&mut self, name: &str, arguments: Value) {
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        assert_eq!(response["result"]["isError"], true, "{name} should fail");
    }

    fn send(&mut self, value: Value) {
        let stdin = self.stdin.as_mut().expect("open child stdin");
        serde_json::to_writer(&mut *stdin, &value).expect("write JSON-RPC request");
        stdin.write_all(b"\n").expect("terminate request");
        stdin.flush().expect("flush request");
    }

    fn finish(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().expect("wait for mcdh-mcp");
        assert!(status.success());
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("child stderr")
            .read_to_string(&mut stderr)
            .expect("read stderr");
        assert!(stderr.contains("started on stdio"));
    }
}

#[test]
fn initializes_lists_strict_schemas_and_calls_every_tool() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("组件库");
    let copies = temp.path().join("副本");
    let moved = temp.path().join("移动");
    let imports = temp.path().join("导入");
    let exports = temp.path().join("导出");
    let mcs_addon = temp.path().join("MCStudioDownload/work/account/Cpp/AddOn");
    for directory in [&library, &copies, &moved, &imports, &exports, &mcs_addon] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let path_text = |path: &Path| path.to_string_lossy().into_owned();
    let mut client = McpClient::start(&temp.path().join("state"));

    let initialized = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "mcdh-test", "version": "1.0"}
        }),
    );
    assert_eq!(initialized["result"]["serverInfo"]["name"], "mcdh");
    client.notify("notifications/initialized");

    let listed = client.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<HashSet<_>>();
    let expected = HashSet::from([
        "list_components",
        "get_component",
        "refresh_components",
        "list_sources",
        "add_single_component",
        "add_library",
        "add_mcs_path",
        "rescan_mcs_paths",
        "remove_source",
        "get_settings",
        "set_settings",
        "create_component",
        "import_component",
        "copy_component",
        "move_component",
        "export_component",
        "set_component_tags",
        "regenerate_manifest_uuids",
        "bump_manifest_version",
        "open_component_directory",
        "open_component_in_vscode",
    ]);
    assert_eq!(names, expected);
    assert!(
        tools
            .iter()
            .all(|tool| { tool["inputSchema"]["additionalProperties"].as_bool() == Some(false) })
    );

    let library_source = client.call("add_library", json!({"path": path_text(&library)}));
    let library_source_id = library_source["id"].as_str().unwrap().to_owned();
    let mcs_sources = client.call("add_mcs_path", json!({"path": path_text(&mcs_addon)}));
    let mcs_source_id = mcs_sources[0]["id"].as_str().unwrap().to_owned();
    client.call("rescan_mcs_paths", json!({}));
    let default_settings = client.call("get_settings", json!({}));
    assert_eq!(default_settings["theme"], "system");
    let saved_settings = client.call(
        "set_settings",
        json!({
            "developer_nickname": "协议开发者",
            "developer_account": "protocol@local.invalid",
            "developer_user_id": "42",
            "default_destination": path_text(&library),
            "theme": "dark"
        }),
    );
    assert_eq!(saved_settings["developer_nickname"], "协议开发者");
    assert_eq!(saved_settings["theme"], "dark");
    let created = client.call(
        "create_component",
        json!({
            "name": "协议测试模组",
            "kind": "addon",
            "destination": path_text(&library),
            "mcs_compatible": false
        }),
    );
    let created_path = created["actual_path"].as_str().unwrap().to_owned();
    let single_source = client.call("add_single_component", json!({"path": created_path}));
    let single_source_id = single_source["id"].as_str().unwrap().to_owned();

    let refreshed = client.call("refresh_components", json!({}));
    let component_id = refreshed["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| {
            component["name"]
                .as_str()
                .is_some_and(|name| name.contains("协议测试"))
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let components = client.call("list_components", json!({}));
    assert!(
        components
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == component_id)
    );
    let component = client.call("get_component", json!({"component_id": component_id}));
    assert_eq!(component["kind"], "addon");
    client.call(
        "set_component_tags",
        json!({"component_id": component_id, "tags": ["测试", "自动化"]}),
    );
    client.call(
        "regenerate_manifest_uuids",
        json!({"component_id": component_id}),
    );
    client.call(
        "bump_manifest_version",
        json!({"component_id": component_id, "part": "patch"}),
    );
    client.call(
        "copy_component",
        json!({
            "component_id": component_id,
            "destination": path_text(&copies),
            "identity_policy": "regenerate"
        }),
    );
    let exported = client.call(
        "export_component",
        json!({"component_id": component_id, "destination": path_text(&exports)}),
    );
    let archive_path = exported["actual_path"].as_str().unwrap().to_owned();
    client.call(
        "import_component",
        json!({
            "source": archive_path,
            "destination": path_text(&imports),
            "identity_policy": "regenerate"
        }),
    );
    client.call(
        "move_component",
        json!({"component_id": component_id, "destination": path_text(&moved)}),
    );
    let sources = client.call("list_sources", json!({}));
    assert!(sources.as_array().unwrap().len() >= 3);

    client.call_error(
        "open_component_directory",
        json!({"component_id": "missing-component"}),
    );
    client.call_error(
        "open_component_in_vscode",
        json!({"component_id": "missing-component"}),
    );
    client.call("remove_source", json!({"source_id": single_source_id}));
    client.call("remove_source", json!({"source_id": library_source_id}));
    client.call("remove_source", json!({"source_id": mcs_source_id}));
    client.finish();
}
