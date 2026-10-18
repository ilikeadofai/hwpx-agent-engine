//! Real-process tests for `hae-mcp` (plan 2-2): JSON-RPC framing, tool list,
//! tool calls and the duplicate-edit guard.

mod common;

use common::{first_code, McpClient, TempDir};
use hae_exec::command_list;
use serde_json::{json, Value};

#[test]
fn initialize_ping_and_tools_list_cover_every_registry_command() {
    let t = TempDir::new("list");
    let mut mcp = McpClient::start_raw(t.path());

    let init = mcp.rpc(
        "initialize",
        json!({ "protocolVersion": "1999-01-01", "capabilities": {}, "clientInfo": {} }),
    );
    // An unknown requested version is answered with one the server supports.
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(init["result"]["serverInfo"]["name"], "hae-mcp");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // Notifications get no reply: the next reply belongs to the ping.
    mcp.notify("notifications/initialized", json!({}));
    let ping = mcp.rpc("ping", json!({}));
    assert_eq!(ping["result"], json!({}));

    let list = mcp.rpc("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    let commands = command_list();
    assert_eq!(tools.len(), commands.len());
    for (tool, command) in tools.iter().zip(&commands) {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(
            name,
            command
                .name
                .replace('.', "_")
                .replace("setText", "set_text")
                .replace("setScript", "set_script")
                .replace("setProperties", "set_properties")
                .replace("replaceAll", "replace_all")
        );
        assert_eq!(name, command.mcp_tool_name);
        assert!(
            tool["description"].as_str().unwrap().len() > 20,
            "{name} needs a real description"
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        // The agent never sees writer or sequence fields.
        let props = schema["properties"].as_object().unwrap();
        assert!(!props.contains_key("writerId") && !props.contains_key("requestSequence"));
        assert!(!props.contains_key("sessionId"));
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert!(!required.contains(&"documentId"), "{name}");
        assert_eq!(
            props.contains_key("documentId"),
            command.requires_document,
            "{name}"
        );
        assert_eq!(
            props.contains_key("expectedVersion"),
            command.requires_expected_version,
            "{name}"
        );
        assert_eq!(
            props.contains_key("dryRun"),
            command.supports_dry_run,
            "{name}"
        );
        assert_eq!(
            required.contains(&"expectedVersion"),
            command.requires_expected_version
        );
    }
    let replace = tools.iter().find(|t| t["name"] == "text_replace").unwrap();
    let props = &replace["inputSchema"]["properties"];
    for key in [
        "target",
        "range",
        "text",
        "documentId",
        "expectedVersion",
        "dryRun",
    ] {
        assert!(props.get(key).is_some(), "text_replace schema lacks {key}");
    }
    assert_eq!(replace["inputSchema"]["additionalProperties"], json!(false));

    let (exit, stderr) = mcp.finish();
    assert_eq!(exit, 0);
    assert!(stderr.contains("hae-mcp:"));
}

#[test]
fn protocol_errors_are_json_rpc_errors_and_the_server_keeps_going() {
    let t = TempDir::new("errors");
    let mut mcp = McpClient::start(t.path());

    let parse = mcp.send_raw(b"{broken");
    assert_eq!(parse["error"]["code"], -32700);
    assert_eq!(parse["id"], Value::Null);

    let batch = mcp.send_raw(b"[]");
    assert_eq!(batch["error"]["code"], -32600);

    let method = mcp.rpc("resources/list", json!({}));
    assert_eq!(method["error"]["code"], -32601);

    let unknown = mcp.rpc(
        "tools/call",
        json!({ "name": "no_such_tool", "arguments": {} }),
    );
    assert_eq!(unknown["error"]["code"], -32602, "{unknown}");
    assert!(unknown.get("result").is_none());

    let bad_args = mcp.rpc(
        "tools/call",
        json!({ "name": "command_list", "arguments": 5 }),
    );
    assert_eq!(bad_args["error"]["code"], -32602);

    // An oversize message is refused and the stream stays in sync.
    let huge = "x".repeat(1024 * 1024 + 1);
    let refused = mcp.send_raw(huge.as_bytes());
    assert_eq!(refused["error"]["code"], -32600);
    let still_alive = mcp.rpc("ping", json!({}));
    assert_eq!(still_alive["result"], json!({}));

    // Tool-level failures are results with isError, not protocol errors.
    let (envelope, is_error) = mcp.call_tool("document_inspect", json!({}));
    assert!(is_error);
    assert_eq!(envelope["ok"], false);
    let (bad_type, is_error) = mcp.call_tool("text_replace", json!({ "expectedVersion": "zero" }));
    assert!(is_error);
    assert_eq!(first_code(&bad_type), "INVALID_REQUEST");
    assert_eq!(mcp.finish().0, 0);
}

#[test]
fn same_edit_twice_gives_version_conflict_and_numbering_stays_in_sync() {
    let t = TempDir::new("twice");
    let path = t.with_corpus("bodojaryo-reference.hwpx");
    let mut mcp = McpClient::start(t.path());

    let (open, is_error) = mcp.call_tool("document_open", json!({ "path": path }));
    assert!(!is_error, "{open}");
    let doc = open["result"]["documentId"].as_str().unwrap().to_string();
    assert_eq!(open["result"]["revision"], 0);

    let (query, _) = mcp.call_tool(
        "element_query",
        json!({ "documentId": doc, "kind": "paragraph", "textContains": "보도자료" }),
    );
    let target = query["result"]["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let edit = json!({
        "documentId": doc,
        "expectedVersion": 0,
        "target": target,
        "range": { "start": 0, "end": 2 },
        "text": "공지",
    });

    let (first, is_error) = mcp.call_tool("text_replace", edit.clone());
    assert!(!is_error, "{first}");
    assert_eq!(first["result"]["outcome"], "applied");
    assert_eq!(first["result"]["afterVersion"], 1);
    assert_eq!(first["replayed"], false);

    // The agent repeats the identical call. The adapter numbered it as a new
    // request, so this is not a replay; the revision moved, hence the conflict.
    let (second, is_error) = mcp.call_tool("text_replace", edit);
    assert!(is_error);
    assert_eq!(first_code(&second), "VERSION_CONFLICT");
    assert_eq!(second["replayed"], false);

    // Failed, dry-run and read calls leave the numbering in sync: a later edit works.
    let (dry, _) = mcp.call_tool(
        "text_replace",
        json!({
            "documentId": doc, "expectedVersion": 1, "dryRun": true, "target": target,
            "range": { "start": 0, "end": 2 }, "text": "안내",
        }),
    );
    assert_eq!(dry["ok"], true, "{dry}");
    let (third, is_error) = mcp.call_tool(
        "text_replace",
        json!({
            "documentId": doc, "expectedVersion": 1, "target": target,
            "range": { "start": 0, "end": 2 }, "text": "안내",
        }),
    );
    assert!(!is_error, "{third}");
    assert_eq!(third["result"]["afterVersion"], 2);

    // History tools take the same envelope fields; undo is a new revision.
    let (undo, is_error) = mcp.call_tool(
        "history_undo",
        json!({ "documentId": doc, "expectedVersion": 2 }),
    );
    assert!(!is_error, "{undo}");
    assert_eq!(undo["result"]["afterVersion"], 3);

    // Rejected by the adapter (wrong type) before it takes a sequence number.
    let (rejected, is_error) = mcp.call_tool(
        "history_redo",
        json!({ "documentId": doc, "expectedVersion": -1 }),
    );
    assert!(is_error);
    assert_eq!(first_code(&rejected), "INVALID_REQUEST");
    let (redo, is_error) = mcp.call_tool(
        "history_redo",
        json!({ "documentId": doc, "expectedVersion": 3 }),
    );
    assert!(!is_error, "{redo}");
    assert_eq!(redo["result"]["afterVersion"], 4);

    // Extra properties are refused by the strict command schema.
    let (extra, is_error) = mcp.call_tool(
        "element_get",
        json!({ "documentId": doc, "target": target, "bogus": true }),
    );
    assert!(is_error);
    assert_eq!(first_code(&extra), "INVALID_REQUEST");
    assert_eq!(mcp.finish().0, 0);
}

#[test]
fn every_command_has_a_description_row() {
    for command in command_list() {
        let text = hae_mcp::descriptions::description(command.name)
            .unwrap_or_else(|| panic!("no description row for {}", command.name));
        assert!(!text.is_empty());
    }
    assert_eq!(
        hae_mcp::descriptions::TABLE.len(),
        command_list().len(),
        "table has a row that is not a registry command"
    );
}

#[test]
fn optional_document_id_never_redirects_an_edit_to_the_open_document() {
    let t = TempDir::new("document-id");
    let path = t.with_corpus("bodojaryo-reference.hwpx");
    let mut mcp = McpClient::start(t.path());
    let (missing, is_error) = mcp.call_tool("document_inspect", json!({}));
    assert!(is_error);
    assert_eq!(first_code(&missing), "SESSION_REQUIRED");

    let (open, is_error) = mcp.call_tool("document_open", json!({ "path": path }));
    assert!(!is_error, "{open}");
    let doc = open["result"]["documentId"].as_str().unwrap();
    let (query, is_error) = mcp.call_tool(
        "element_query",
        json!({ "kind": "paragraph", "textContains": "보도자료" }),
    );
    assert!(!is_error, "{query}");
    let target = query["result"]["matches"][0]["nodeId"].clone();
    let (before, is_error) = mcp.call_tool("element_get", json!({ "target": target }));
    assert!(!is_error, "{before}");
    let mut edit = json!({
        "documentId": format!("{doc}-wrong"), "expectedVersion": 0,
        "target": target, "range": { "start": 0, "end": 2 }, "text": "공지",
    });
    let (rejected, is_error) = mcp.call_tool("text_replace", edit.clone());
    assert!(is_error);
    assert_eq!(first_code(&rejected), "TARGET_NOT_FOUND");
    let (after, is_error) = mcp.call_tool(
        "element_get",
        json!({ "documentId": doc, "target": target }),
    );
    assert!(!is_error, "{after}");
    assert_eq!(before["result"], after["result"]);
    edit.as_object_mut().unwrap().remove("documentId");
    let (applied, is_error) = mcp.call_tool("text_replace", edit);
    assert!(!is_error, "{applied}");
    assert_eq!(applied["result"]["beforeVersion"], 0);
    assert_eq!(applied["result"]["afterVersion"], 1);
    assert_eq!(mcp.finish().0, 0);
}

#[test]
fn equation_tools_stdio_list_get_edit_dry_run_batch_and_exact_undo() {
    let t = TempDir::new("equations");
    let input = t.path().join("equations.hwpx");
    let original = include_bytes!("../../../tests/fixtures/editing/equations.hwpx");
    std::fs::write(&input, original).unwrap();
    let mut mcp = McpClient::start(t.path());
    let (opened, err) = mcp.call_tool("document_open", json!({"path":input}));
    assert!(!err, "{opened}");
    let (list, err) = mcp.call_tool("equation_list", json!({"limit":1}));
    assert!(!err, "{list}");
    let id = list["result"]["items"][0]["nodeId"].clone();
    let (get, err) = mcp.call_tool("equation_get", json!({"target":id}));
    assert!(!err, "{get}");
    assert!(get["result"]["equation"]["paragraphTextPreview"].is_string());
    let (dry, err) = mcp.call_tool(
        "equation_set_script",
        json!({"target":id,"script":"x < y & z","expectedVersion":0,"dryRun":true}),
    );
    assert!(!err, "{dry}");
    assert_eq!(dry["result"]["afterVersion"], 0);
    let (script, err) = mcp.call_tool(
        "equation_set_script",
        json!({"target":id,"script":"x < y & z","expectedVersion":0}),
    );
    assert!(!err, "{script}");
    let (props, err) = mcp.call_tool(
        "equation_set_properties",
        json!({"target":id,"props":{"baseLine":70},"expectedVersion":1}),
    );
    assert!(!err, "{props}");
    let (batch, err) = mcp.call_tool("command_batch", json!({"expectedVersion":2,"steps":[{"command":"equation.setScript","params":{"target":id,"script":"x+1"}},{"command":"equation.setProperties","params":{"target":id,"props":{"textColor":"#123456"}}}]}));
    assert!(!err, "{batch}");
    for version in 3..=5 {
        let (undo, err) = mcp.call_tool("history_undo", json!({"expectedVersion":version}));
        assert!(!err, "{undo}");
    }
    let path = t.path().join("restored.hwpx");
    let (export, err) = mcp.call_tool("document_export", json!({"path":path}));
    assert!(!err, "{export}");
    assert_eq!(std::fs::read(path).unwrap(), original);
    let (code, stderr) = mcp.finish();
    assert_eq!(code, 0, "{stderr}");
}
