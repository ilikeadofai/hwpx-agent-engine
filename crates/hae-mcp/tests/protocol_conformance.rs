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
fn discovery_includes_required_cache_fields_without_initialization() {
    let t = TempDir::new("discover-cache");
    let mut c = McpClient::start_raw(t.path());
    let reply = c.rpc("server/discover", json!({"_meta":meta("2026-07-28")}));
    assert_eq!(reply["jsonrpc"], "2.0");
    let result = &reply["result"];
    assert_eq!(result["resultType"], "complete");
    assert!(result["supportedVersions"]
        .as_array()
        .unwrap()
        .contains(&json!("2026-07-28")));
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["ttlMs"].as_u64().is_some());
    assert!(["public", "private"].contains(&result["cacheScope"].as_str().unwrap()));
    let info = &result["_meta"]["io.modelcontextprotocol/serverInfo"];
    assert_eq!(info["name"], "hae-mcp");
    assert!(info["version"].is_string());
    assert!(c.rpc("tools/list", json!({"_meta":meta("2026-07-28")}))["result"]["tools"].is_array());
    assert_eq!(c.finish().0, 0);
}

#[test]
fn cache_metadata_is_only_on_modern_cacheable_results() {
    let t = TempDir::new("cache-revisions");
    let path = t.with_corpus("ilban-gian.hwpx");
    for version in hae_mcp::SUPPORTED_PROTOCOL_VERSIONS {
        let modern = *version == "2026-07-28";
        let mut c = McpClient::start_raw(t.path());
        let params = if modern {
            json!({"_meta":meta(version)})
        } else {
            let init = c.rpc("initialize", json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}));
            for field in ["ttlMs", "cacheScope", "resultType"] {
                assert!(init["result"].get(field).is_none(), "{version}: {init}");
            }
            c.notify("notifications/initialized", json!({}));
            json!({})
        };
        let list = c.rpc("tools/list", params.clone());
        let result = &list["result"];
        assert_eq!(
            result["tools"].as_array().unwrap().len(),
            hae_exec::command_list().len()
        );
        if modern {
            assert_eq!(result["resultType"], "complete");
            assert!(
                result["ttlMs"].as_u64().is_some_and(|ttl| ttl > 0),
                "{result}"
            );
            assert_eq!(result["cacheScope"], "public");
        } else {
            for field in ["ttlMs", "cacheScope", "resultType"] {
                assert!(result.get(field).is_none(), "{version}: {field}");
            }
        }
        let mut open = params.clone();
        open["name"] = json!("document_open");
        open["arguments"] = json!({"path":path});
        let opened = c.rpc("tools/call", open);
        assert_eq!(opened["result"]["isError"], false, "{opened}");
        let ping = c.rpc("ping", params.clone());
        for reply in [&opened, &ping] {
            assert!(reply["result"].get("ttlMs").is_none(), "{reply}");
            assert!(reply["result"].get("cacheScope").is_none(), "{reply}");
            assert_eq!(reply["result"].get("resultType").is_some(), modern);
        }
        // Public caching is valid because opening a document cannot personalize
        // the registry, schemas, or descriptions advertised by tools/list.
        assert_eq!(c.rpc("tools/list", params)["result"], list["result"]);
        assert_eq!(c.finish().0, 0);
    }
}

#[test]
fn document_handle_guidance_and_recovery_match_each_revision() {
    let t = TempDir::new("handle-guidance");
    let path = t.with_corpus("ilban-gian.hwpx");
    for version in hae_mcp::SUPPORTED_PROTOCOL_VERSIONS {
        let modern = *version == "2026-07-28";
        let mut c = McpClient::start_raw(t.path());
        let params = if modern {
            json!({"_meta":meta(version)})
        } else {
            json!({})
        };
        let discovery = if modern {
            c.rpc("server/discover", params.clone())
        } else {
            let init = c.rpc("initialize", json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}));
            c.notify("notifications/initialized", json!({}));
            init
        };
        let instructions = discovery["result"]["instructions"].as_str().unwrap();
        for term in [
            "documentId",
            "2026-07-28",
            "required",
            "initialize",
            "optional",
        ] {
            assert!(
                instructions.contains(term),
                "missing {term}: {instructions}"
            );
        }
        let list = c.rpc("tools/list", params.clone());
        for tool in list["result"]["tools"].as_array().unwrap() {
            if let Some(property) = tool["inputSchema"]["properties"].get("documentId") {
                let required = tool["inputSchema"]["required"]
                    .as_array()
                    .is_some_and(|r| r.contains(&json!("documentId")));
                assert_eq!(required, modern, "{} {version}", tool["name"]);
                let guidance = property["description"].as_str().unwrap();
                assert!(
                    guidance.contains(if modern { "Required" } else { "Optional" }),
                    "{guidance}"
                );
                let description = tool["description"].as_str().unwrap();
                assert!(
                    description.contains(if modern {
                        "Required documentId"
                    } else {
                        "Optional documentId"
                    }),
                    "{description}"
                );
            }
        }
        let mut call = params.clone();
        call["name"] = json!("document_open");
        call["arguments"] = json!({"path":path});
        let open = c.rpc("tools/call", call.clone());
        assert_eq!(open["result"]["isError"], false, "{open}");
        let doc = &open["result"]["structuredContent"]["result"]["documentId"];
        call["name"] = json!("document_inspect");
        call["arguments"] = json!({"limit":1});
        let omitted = c.rpc("tools/call", call.clone());
        assert_eq!(omitted["result"]["isError"], modern, "{version}: {omitted}");
        if modern {
            let diagnostic = &omitted["result"]["structuredContent"]["diagnostics"][0];
            assert_eq!(diagnostic["code"], "INVALID_REQUEST");
            let message = diagnostic["message"].as_str().unwrap();
            for term in ["document_open", "documentId", "arguments"] {
                assert!(message.contains(term), "{message}");
            }
        }
        call["arguments"]["documentId"] = doc.clone();
        assert_eq!(c.rpc("tools/call", call)["result"]["isError"], false);
        assert_eq!(c.finish().0, 0);
    }
}

fn assert_standard_formats(value: &Value, path: &str) {
    match value {
        Value::Object(object) => {
            if let Some(format) = object.get("format").and_then(Value::as_str) {
                assert!(
                    [
                        "date-time",
                        "date",
                        "time",
                        "duration",
                        "email",
                        "idn-email",
                        "hostname",
                        "idn-hostname",
                        "ipv4",
                        "ipv6",
                        "uri",
                        "uri-reference",
                        "iri",
                        "iri-reference",
                        "uuid",
                        "uri-template",
                        "json-pointer",
                        "relative-json-pointer",
                        "regex"
                    ]
                    .contains(&format),
                    "nonstandard format {format} at {path}"
                );
            }
            for (key, child) in object {
                // These keywords contain instance data, not schemas.
                if !["default", "examples", "const", "enum"].contains(&key.as_str()) {
                    assert_standard_formats(child, &format!("{path}/{key}"));
                }
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                assert_standard_formats(child, &format!("{path}/{index}"));
            }
        }
        _ => {}
    }
}

#[test]
fn all_advertised_and_registry_schemas_use_only_standard_formats() {
    for descriptor in hae_exec::command_list() {
        assert_standard_formats(
            &descriptor.input_schema,
            &format!("{}/input", descriptor.name),
        );
        assert_standard_formats(
            &descriptor.output_schema,
            &format!("{}/output", descriptor.name),
        );
    }
    let t = TempDir::new("standard-formats");
    let mut c = McpClient::start(t.path());
    for params in [json!({}), json!({"_meta":meta("2026-07-28")})] {
        let list = c.rpc("tools/list", params);
        for tool in list["result"]["tools"].as_array().unwrap() {
            for key in ["inputSchema", "outputSchema"] {
                assert_standard_formats(&tool[key], &format!("{}/{key}", tool["name"]));
            }
        }
    }
    assert_eq!(c.finish().0, 0);
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
    let supported = r["result"]["supportedVersions"].clone();
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
    assert_eq!(r["error"]["data"]["supported"], supported);
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

// Removing the shared font preflight would make these accept oversized folders.
fn doctor_fonts(dir: &std::path::Path) -> (Value, bool) {
    let result = Command::new(HAE_MCP)
        .args([
            "doctor",
            "--input-root",
            dir.to_str().unwrap(),
            "--output-root",
            dir.to_str().unwrap(),
        ])
        .env("HAE_RHWP", "/bin/true")
        .env("HAE_RASTER", "/bin/true")
        .env("HAE_BWRAP", "/bin/true")
        .env("HAE_FONT_DIR", dir)
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    (report["checks"]["fonts"].clone(), result.status.success())
}

#[test]
fn doctor_rejects_oversized_font_payload_before_reading() {
    let t = TempDir::new("doctor-font-size");
    for name in ["a.ttf", "b.OTF", "c.ttc"] {
        std::fs::File::create(t.path().join(name))
            .unwrap()
            .set_len(24 * 1024 * 1024)
            .unwrap();
    }
    let (fonts, success) = doctor_fonts(t.path());
    assert!(!success);
    assert_eq!(fonts["ok"], false);
    assert_eq!(fonts["fileCount"], 3);
    assert_eq!(fonts["totalBytes"], 75_497_472u64);
    assert_eq!(fonts["measurementsComplete"], true);
    assert!(fonts["guidance"].as_str().unwrap().contains("HAE_FONT_DIR"));
}

#[test]
fn doctor_accepts_small_folder_and_warns_without_korean_filename() {
    let t = TempDir::new("doctor-font-small");
    std::fs::write(t.path().join("Latin.ttf"), b"synthetic").unwrap();
    std::fs::File::create(t.path().join("ignored.bin"))
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let (fonts, success) = doctor_fonts(t.path());
    assert!(success);
    assert_eq!(fonts["fileCount"], 1);
    assert_eq!(fonts["totalBytes"], 9);
    assert!(fonts["warning"].is_string());
    std::fs::rename(
        t.path().join("Latin.ttf"),
        t.path().join("NotoSansKR-Regular.ttf"),
    )
    .unwrap();
    let (fonts, success) = doctor_fonts(t.path());
    assert!(success);
    assert!(fonts["warning"].is_null());
}

#[test]
fn doctor_empty_font_folder_warns() {
    let t = TempDir::new("doctor-font-empty");
    let (fonts, success) = doctor_fonts(t.path());
    assert!(success);
    assert_eq!(fonts["ok"], true);
    assert_eq!(fonts["fileCount"], 0);
    assert_eq!(fonts["totalBytes"], 0);
    assert!(fonts["warning"].is_string());
}

#[test]
fn doctor_rejects_single_font_over_per_file_limit() {
    let t = TempDir::new("doctor-font-single");
    std::fs::File::create(t.path().join("large.otc"))
        .unwrap()
        .set_len(33 * 1024 * 1024)
        .unwrap();
    let (fonts, success) = doctor_fonts(t.path());
    assert!(!success);
    assert_eq!(fonts["fileCount"], 1);
    assert_eq!(fonts["totalBytes"], 34_603_008u64);
}

#[test]
fn doctor_rejects_too_many_fonts_with_partial_measurements() {
    let t = TempDir::new("doctor-font-count");
    for i in 0..4097 {
        std::fs::write(t.path().join(format!("{i:04}.ttf")), b"x").unwrap();
    }
    let (fonts, success) = doctor_fonts(t.path());
    assert!(!success);
    assert_eq!(fonts["fileCount"], 4097);
    assert_eq!(fonts["totalBytes"], 4097);
    assert_eq!(fonts["measurementsComplete"], false);
}
