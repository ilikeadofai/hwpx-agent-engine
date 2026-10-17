//! Regression tests over an actual stdio child process, never a mock transport.
mod common;
use common::{McpClient, TempDir, HAE_MCP};
use serde_json::{json, Value};
use std::process::Command;

fn meta(version: &str) -> Value {
    json!({"io.modelcontextprotocol/protocolVersion":version,
        "io.modelcontextprotocol/clientCapabilities":{},
        "io.modelcontextprotocol/clientInfo":{"name":"conformance","version":"1"}})
}

#[test]
fn ids_are_exact_and_invalid_ids_never_echo_arbitrary_values() {
    let t = TempDir::new("ids");
    let mut c = McpClient::start(t.path());
    for id in [
        "123456789012345678901234567890",
        "-123456789012345678901234567890",
        "0",
        "\"한글-id\"",
    ] {
        let line = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
        let r = c.send_raw(line.as_bytes());
        assert_eq!(r["id"].to_string(), id);
        assert!(r.get("error").is_none(), "{r}");
    }
    for id in ["null", "{}", "[]", "true", "1.5"] {
        let r = c.send_raw(format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#).as_bytes());
        assert_eq!(r["id"], Value::Null);
        assert_eq!(r["error"]["code"], -32600, "{r}");
    }
    assert_eq!(c.finish().0, 0);
}

#[test]
fn legacy_initialization_and_all_supported_revisions() {
    let t = TempDir::new("versions");
    let mut c = McpClient::start_raw(t.path());
    assert_eq!(c.rpc("ping", json!({}))["result"], json!({}));
    for method in ["tools/list", "tools/call"] {
        assert!(c.rpc(method, json!({})).get("error").is_some());
    }
    assert_eq!(c.rpc("initialize", json!({}))["error"]["code"], -32602);
    assert_eq!(c.finish().0, 0);
    for version in [
        "2025-11-25",
        "2025-06-18",
        "2025-03-26",
        "2024-11-05",
        "1999-01-01",
        "2026-07-28",
    ] {
        let mut c = McpClient::start_raw(t.path());
        let r = c.rpc("initialize", json!({"protocolVersion":version,"clientInfo":{"name":"test","version":"1"},"capabilities":{}}));
        let expected = if ["1999-01-01", "2026-07-28"].contains(&version) {
            "2025-11-25"
        } else {
            version
        };
        assert_eq!(r["result"]["protocolVersion"], expected);
        c.notify("notifications/initialized", json!({}));
        c.notify("notifications/cancelled", json!({"requestId":999}));
        c.notify("notifications/unknown", json!({}));
        assert!(c.rpc("tools/list", json!({}))["result"]["tools"].is_array());
        assert_eq!(c.finish().0, 0);
    }
}

#[test]
fn modern_requests_negotiate_per_request_and_require_explicit_document_handles() {
    let t = TempDir::new("modern");
    let mut c = McpClient::start_raw(t.path());
    let r = c.rpc("server/discover", json!({"_meta":meta("2026-07-28")}));
    assert_eq!(r["result"]["resultType"], "complete");
    assert_eq!(r["result"]["supportedVersions"][0], "2026-07-28");
    let tools = c.rpc("tools/list", json!({"_meta":meta("2026-07-28")}));
    let inspect = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "document_inspect")
        .unwrap();
    assert!(inspect["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("documentId")));
    assert_eq!(
        r["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["version"],
        "0.2.0"
    );
    let r = c.rpc("tools/list", json!({"_meta":meta("1900-01-01")}));
    assert_eq!(r["error"]["code"], -32022);
    assert_eq!(r["error"]["data"]["requested"], "1900-01-01");
    let r = c.rpc(
        "tools/list",
        json!({"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}),
    );
    assert_eq!(r["error"]["code"], -32602);
    // No modern request may initialize the implicit legacy session.
    assert!(c.rpc("tools/list", json!({})).get("error").is_some());
    let path = t.with_corpus("ilban-gian.hwpx");
    let r = c.rpc(
        "tools/call",
        json!({"_meta":meta("2026-07-28"),"name":"document_open","arguments":{"path":path}}),
    );
    let doc = r["result"]["structuredContent"]["result"]["documentId"]
        .as_str()
        .unwrap();
    let r = c.rpc("tools/call", json!({"_meta":meta("2026-07-28"),"name":"document_inspect","arguments":{"documentId":doc,"limit":1}}));
    assert_eq!(r["result"]["isError"], false);
    let r = c.rpc(
        "tools/call",
        json!({"_meta":meta("2026-07-28"),"name":"document_inspect","arguments":{}}),
    );
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(c.finish().0, 0);
}

#[test]
fn every_tool_has_output_schema_annotations_and_matching_structured_content() {
    let t = TempDir::new("schemas");
    let mut c = McpClient::start(t.path());
    let list = c.rpc("tools/list", json!({}));
    for tool in list["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["outputSchema"]["type"], "object", "{}", tool["name"]);
        for key in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert!(
                tool["annotations"][key].is_boolean(),
                "{} {key}",
                tool["name"]
            );
        }
        let r = c.rpc("tools/call", json!({"name":tool["name"],"arguments":{}}));
        let result = &r["result"];
        assert!(result["structuredContent"].is_object(), "{r}");
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, result["structuredContent"]);
    }
    assert_eq!(c.finish().0, 0);
}

#[test]
fn malformed_protocol_and_actionable_input_errors_are_distinct() {
    let t = TempDir::new("classification");
    let mut c = McpClient::start(t.path());
    for params in [
        json!({}),
        json!({"name":"command_list","arguments":null}),
        json!({"name":"command_list","arguments":[]}),
        json!({"name":"missing","arguments":{}}),
    ] {
        assert_eq!(c.rpc("tools/call", params)["error"]["code"], -32602);
    }
    for args in [
        json!({}),
        json!({"path":1}),
        json!({"path":"/etc/passwd"}),
        json!({"path":"/missing/a.hwpx"}),
        json!({"path":"/tmp/a.hwpx","unknown":true}),
    ] {
        let r = c.rpc(
            "tools/call",
            json!({"name":"document_open","arguments":args}),
        );
        assert!(r.get("error").is_none(), "{r}");
        assert_eq!(r["result"]["isError"], true);
    }
    assert_eq!(c.finish().0, 0);
}

#[test]
fn doctor_checks_dependencies_and_roots_without_starting_stdio() {
    let t = TempDir::new("doctor");
    let r = Command::new(HAE_MCP)
        .args([
            "--input-root",
            t.path().to_str().unwrap(),
            "--output-root",
            t.path().to_str().unwrap(),
            "doctor",
        ])
        .env_remove("HAE_RHWP")
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(report["version"], "0.2.0");
    for key in [
        "rhwp",
        "raster",
        "bwrap",
        "fonts",
        "inputRoot",
        "outputRoot",
    ] {
        assert!(report["checks"][key]["ok"].is_boolean(), "{key}: {report}");
    }
    assert_eq!(report["checks"]["inputRoot"]["ok"], true);
    assert_eq!(report["checks"]["outputRoot"]["ok"], true);
    assert_eq!(report["checks"]["rhwp"]["ok"], false);
    assert_eq!(r.status.code(), Some(1));
}

#[test]
fn doctor_reports_healthy_paths_and_invalid_roots() {
    let t = TempDir::new("doctor-paths");
    for missing in [false, true] {
        let output = if missing {
            t.path().join("missing")
        } else {
            t.path().to_path_buf()
        };
        let result = Command::new(HAE_MCP)
            .args([
                "doctor",
                "--input-root",
                t.path().to_str().unwrap(),
                "--output-root",
                output.to_str().unwrap(),
            ])
            .env("HAE_RHWP", "/bin/true")
            .env("HAE_RASTER", "/bin/true")
            .env("HAE_BWRAP", "/bin/true")
            .env("HAE_FONT_DIR", t.path())
            .output()
            .unwrap();
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["ok"], !missing);
        assert_eq!(report["checks"]["outputRoot"]["ok"], !missing);
        assert_eq!(result.status.success(), !missing);
        assert!(!std::fs::read_dir(t.path()).unwrap().any(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".hae-doctor-")));
    }
}

#[test]
fn only_march_2025_revision_accepts_protocol_batches_after_handshake() {
    let t = TempDir::new("batch-versions");
    let mut c = McpClient::start_raw(t.path());
    c.rpc("initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}));
    c.notify("notifications/initialized", json!({}));
    let response = c.send_raw(br#"[{"jsonrpc":"2.0","id":"p1","method":"ping"},{"jsonrpc":"2.0","method":"notifications/unknown","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},{"jsonrpc":"2.0","id":"p2","method":"tools/list"}]"#);
    assert_eq!(response.as_array().unwrap().len(), 2);
    assert_eq!(response[0]["id"], "p1");
    assert!(response[1]["result"]["tools"].is_array());
    c.notify("notifications/unknown", json!({}));
    assert_eq!(c.send_raw(b"[]")["error"]["code"], -32600);
    let modern = json!([{"jsonrpc":"2.0","id":"m","method":"tools/list","params":{"_meta":meta("2026-07-28")}}]);
    let rejected_modern = c.send_raw(modern.to_string().as_bytes());
    assert_eq!(rejected_modern[0]["error"]["code"], -32600);
    assert_eq!(rejected_modern[0]["id"], "m");
    assert_eq!(c.finish().0, 0);
}

#[test]
fn legacy_batch_wire_budget_preserves_each_request_id() {
    let t = TempDir::new("batch-budget");
    let mut c = McpClient::start_raw(t.path());
    c.rpc("initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}));
    c.notify("notifications/initialized", json!({}));
    let batch: Vec<_> = (0..128)
        .map(|id| json!({"jsonrpc":"2.0","id":format!("batch-{id}"),"method":"tools/list"}))
        .collect();
    let r = c.send_raw(serde_json::to_string(&batch).unwrap().as_bytes());
    assert!(r.is_array(), "large batch must retain per-request replies");
    assert_eq!(r.as_array().unwrap().len(), batch.len());
    assert!(serde_json::to_vec(&r).unwrap().len() <= hae_jsonl::MAX_RESPONSE_BYTES);
    let mut limited = 0;
    for (request, reply) in batch.iter().zip(r.as_array().unwrap()) {
        assert_eq!(request["id"], reply["id"]);
        if reply.get("error").is_some() {
            limited += 1;
            assert_eq!(reply["error"]["code"], -32603);
        } else {
            assert_eq!(reply["result"]["tools"].as_array().unwrap().len(), 22);
        }
    }
    assert!(limited > 0);
    assert_eq!(c.rpc("ping", json!({}))["result"], json!({}));
    assert_eq!(c.finish().0, 0);
}
