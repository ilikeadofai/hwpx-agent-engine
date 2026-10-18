//! hae-mcp: stdio MCP server (JSON-RPC 2.0, one message per line) over the session executor.
//!
//! Every registry command becomes one MCP tool (`cell.setText` → `cell_set_text`).
//! The agent supplies `documentId` (optional after legacy initialize), `expectedVersion`, `dryRun` and the command
//! params only. The adapter issues one writer per connection and numbers
//! sequence-tracked requests itself (spec §6), so a duplicated agent call
//! fails with `VERSION_CONFLICT` instead of being applied twice.

pub mod descriptions;
pub mod doctor;
mod schemas;
use schemas::{output_schema, portable_schema};

use hae_diag::{Code, Diagnostic, EngineError, Stage};
use hae_exec::{
    command_list, CommandDescriptor, RequestEnvelope, ResponseEnvelope, SessionExecutor,
};
use hae_jsonl::{
    read_line_limited, response_too_large, Line, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Newest first. Modern negotiation is per request; legacy fallback uses index 1.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    "2026-07-28",
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];
const MODERN_VERSION: &str = "2026-07-28";
const VERSION_META: &str = "io.modelcontextprotocol/protocolVersion";
const CAPS_META: &str = "io.modelcontextprotocol/clientCapabilities";
const INFO_META: &str = "io.modelcontextprotocol/serverInfo";
// Discovery and the tool registry contain no document or caller-specific data.
// Cache for five minutes; restart/upgrade changes need no permanent cache.
const STATIC_METADATA_TTL_MS: u64 = 300_000;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

pub struct McpServer {
    executor: SessionExecutor,
    writer_id: String,
    next_seq: u64,
    commands: Vec<CommandDescriptor>,
    legacy_version: Option<String>,
    initialized: bool,
    faulted: bool,
}

impl McpServer {
    pub fn new(mut executor: SessionExecutor) -> Result<Self, EngineError> {
        let writer_id = executor.issue_writer()?;
        Ok(Self {
            executor,
            writer_id,
            next_seq: 1,
            commands: command_list(),
            legacy_version: None,
            initialized: false,
            faulted: false,
        })
    }

    /// The `tools/list` result: one tool per registry command.
    pub fn tools_list(&self) -> Value {
        self.tools_list_for(false)
    }

    fn tools_list_for(&self, modern: bool) -> Value {
        let tools: Vec<Value> = self
            .commands
            .iter()
            .map(|d| {
                let mut description = descriptions::description(d.name)
                    .unwrap_or(d.summary)
                    .to_owned();
                if d.requires_document {
                    description.push(' ');
                    description.push_str(descriptions::document_id_property(modern));
                }
                let mut input = input_schema(d);
                if modern && d.requires_document {
                    let root = input.as_object_mut().unwrap();
                    root.entry("required")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .unwrap()
                        .push(json!("documentId"));
                    root["properties"]["documentId"]["description"] =
                        json!(descriptions::document_id_property(true));
                }
                json!({
                    "name": d.mcp_tool_name,
                    "description": description,
                    "inputSchema": input,
                    "outputSchema": output_schema(d),
                    "annotations": d.annotations,
                })
            })
            .collect();
        json!({ "tools": tools })
    }

    /// Handle one inbound line. Returns the line to write back, if any
    /// (notifications get no reply).
    pub fn handle_message(&mut self, line: &[u8]) -> Option<Value> {
        let message: Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(e) => {
                return Some(rpc_error(
                    &Value::Null,
                    &RpcError::new(PARSE_ERROR, format!("parse error: {e}")),
                ))
            }
        };
        if let Some(batch) = message.as_array() {
            if self.legacy_version.as_deref() == Some("2025-03-26")
                && self.initialized
                && !batch.is_empty()
                && batch.len() <= 256
                && !batch
                    .iter()
                    .any(|item| item.get("method").and_then(Value::as_str) == Some("initialize"))
            {
                let mut replies = Vec::new();
                let mut used_bytes = 0;
                // Reserve half the wire budget for bounded error replies and
                // their IDs (all IDs together fit in the 1MiB inbound line).
                // Never collect 256 multi-megabyte results before checking size.
                for item in batch {
                    let reply = if item.is_array()
                        || (item.get("id").is_some()
                            && item
                                .get("params")
                                .and_then(|p| p.get("_meta"))
                                .and_then(|m| m.get(VERSION_META))
                                .is_some())
                    {
                        Some(rpc_error(
                            item.get("id")
                                .filter(|id| valid_id(id))
                                .unwrap_or(&Value::Null),
                            &RpcError::new(
                                INVALID_REQUEST,
                                "nested or modern request in a legacy batch is invalid",
                            ),
                        ))
                    } else {
                        self.handle_message(
                            &serde_json::to_vec(item).expect("parsed JSON serializes"),
                        )
                    };
                    if let Some(reply) = reply {
                        let size = serde_json::to_vec(&reply).expect("reply serializes").len();
                        if size > (MAX_RESPONSE_BYTES / 2).saturating_sub(used_bytes) {
                            replies.push(rpc_error(
                                reply.get("id").unwrap_or(&Value::Null),
                                &RpcError::new(
                                    -32603,
                                    "RESOURCE_LIMIT: batch response budget exceeded",
                                ),
                            ));
                        } else {
                            used_bytes += size;
                            replies.push(reply);
                        }
                    }
                }
                return (!replies.is_empty()).then_some(Value::Array(replies));
            }
        }
        let Some(object) = message.as_object() else {
            let what = if message.is_array() {
                "JSON-RPC batches are not supported; send one message per line"
            } else {
                "a JSON-RPC message must be an object"
            };
            return Some(rpc_error(
                &Value::Null,
                &RpcError::new(INVALID_REQUEST, what),
            ));
        };
        let id = object.get("id").cloned();
        if id.as_ref().is_some_and(|id| !valid_id(id)) {
            return Some(rpc_error(
                &Value::Null,
                &RpcError::new(
                    INVALID_REQUEST,
                    "id must be a string or integer, never null",
                ),
            ));
        }
        let method = object.get("method").and_then(Value::as_str);
        let Some(method) = method else {
            // A response from the client to a server request; this server sends none.
            if object.contains_key("result") || object.contains_key("error") {
                return None;
            }
            return Some(rpc_error(
                id.as_ref().unwrap_or(&Value::Null),
                &RpcError::new(INVALID_REQUEST, "missing method"),
            ));
        };
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(rpc_error(
                id.as_ref().unwrap_or(&Value::Null),
                &RpcError::new(INVALID_REQUEST, "jsonrpc must be \"2.0\""),
            ));
        }
        let Some(id) = id else {
            // Serial execution means cancellation is read only after the active
            // request has completed: completed/unknown request IDs are ignored.
            if method == "notifications/initialized" && self.legacy_version.is_some() {
                self.initialized = true;
            }
            eprintln!("hae-mcp: notification {method}");
            return None;
        };
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        let outcome = guarded(|| self.dispatch(method, &params));
        let outcome = match outcome {
            Ok(result) => result,
            Err(error) => {
                // An unwind may leave executor state uncertain. Fail closed for
                // further work; retain ping/EOF so the client can restart cleanly.
                self.faulted = true;
                eprintln!("hae-mcp: request panicked; executor quarantined");
                Err(error)
            }
        };
        Some(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => rpc_error(&id, &e),
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        if !params.is_object() {
            return Err(RpcError::new(INVALID_PARAMS, "params must be an object"));
        }
        if self.faulted && method != "ping" {
            return Err(RpcError::new(
                -32603,
                "executor quarantined after panic; restart server",
            ));
        }
        let metadata = params.get("_meta");
        if metadata.is_some_and(|m| !m.is_object()) {
            return Err(RpcError::new(INVALID_PARAMS, "_meta must be an object"));
        }
        let modern =
            metadata.is_some_and(|m| m.get(VERSION_META).is_some()) || method == "server/discover";
        if modern {
            let requested = metadata
                .and_then(|m| m.get(VERSION_META))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RpcError::new(
                        INVALID_PARAMS,
                        "modern request requires _meta protocolVersion",
                    )
                })?;
            if requested != MODERN_VERSION {
                return Err(RpcError {
                    code: -32022,
                    message: "Unsupported protocol version".into(),
                    data: Some(
                        json!({"supported":SUPPORTED_PROTOCOL_VERSIONS,"requested":requested}),
                    ),
                });
            }
            if !metadata
                .and_then(|m| m.get(CAPS_META))
                .is_some_and(Value::is_object)
            {
                return Err(RpcError::new(
                    INVALID_PARAMS,
                    "modern request requires _meta clientCapabilities object",
                ));
            }
            if metadata
                .and_then(|m| m.get("io.modelcontextprotocol/clientInfo"))
                .is_some_and(|v| {
                    !v.get("name").is_some_and(Value::is_string)
                        || !v.get("version").is_some_and(Value::is_string)
                })
            {
                return Err(RpcError::new(
                    INVALID_PARAMS,
                    "clientInfo needs string name and version",
                ));
            }
        }
        let mut result = match method {
            "initialize" if !modern => {
                if self.legacy_version.is_some() {
                    return Err(RpcError::new(INVALID_REQUEST, "already initialized"));
                }
                let requested = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        RpcError::new(INVALID_PARAMS, "initialize requires protocolVersion")
                    })?;
                if !params.get("capabilities").is_some_and(Value::is_object)
                    || !params.get("clientInfo").is_some_and(Value::is_object)
                {
                    return Err(RpcError::new(
                        INVALID_PARAMS,
                        "initialize requires capabilities and clientInfo objects",
                    ));
                }
                // initialize is a legacy handshake. Modern clients use _meta.
                let version = if SUPPORTED_PROTOCOL_VERSIONS[1..].contains(&requested) {
                    requested
                } else {
                    SUPPORTED_PROTOCOL_VERSIONS[1]
                };
                self.legacy_version = Some(version.to_owned());
                initialize_result(version)
            }
            "ping" => json!({}),
            "server/discover" if modern => json!({
                "supportedVersions": SUPPORTED_PROTOCOL_VERSIONS,
                "capabilities": capabilities(),
                "instructions": descriptions::server_instructions(),
            }),
            "tools/list" | "tools/call" => {
                if !modern && !self.initialized {
                    return Err(RpcError::new(
                        INVALID_REQUEST,
                        "initialize and notifications/initialized required before tools requests",
                    ));
                }
                if method == "tools/list" {
                    self.tools_list_for(modern)
                } else {
                    self.tools_call(params, modern)?
                }
            }
            other => {
                return Err(RpcError::new(
                    METHOD_NOT_FOUND,
                    format!("method not found: {other}"),
                ))
            }
        };
        if modern {
            result["resultType"] = json!("complete");
            result["_meta"] = json!({ INFO_META: server_info() });
            // Only these implemented methods inherit CacheableResult in
            // 2026-07-28. Legacy responses and tools/call/ping have no hints.
            if matches!(method, "server/discover" | "tools/list") {
                result["ttlMs"] = json!(STATIC_METADATA_TTL_MS);
                result["cacheScope"] = json!("public");
            }
        }
        Ok(result)
    }

    fn tools_call(&mut self, params: &Value, modern: bool) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tools/call needs a string `name`"))?;
        let descriptor = self
            .commands
            .iter()
            .find(|d| d.mcp_tool_name == name)
            .cloned()
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, format!("unknown tool: {name}")))?;
        let arguments = match params.get("arguments") {
            None => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(_) => {
                return Err(RpcError::new(
                    INVALID_PARAMS,
                    "`arguments` must be a JSON object",
                ))
            }
        };
        let response = if modern
            && descriptor.requires_document
            && !arguments.get("documentId").is_some_and(Value::is_string)
        {
            self.invalid_tool(
                &descriptor,
                "Stateless MCP 2026-07-28 calls require documentId: call document_open, then pass its returned documentId in arguments.documentId on every document tool call. Reopen the document after a server restart.".into(),
            )
        } else {
            self.run_tool(&descriptor, arguments)
        };
        if response.ok {
            eprintln!("hae-mcp: {} ok", descriptor.name);
        } else {
            for d in &response.diagnostics {
                eprintln!(
                    "hae-mcp: {} failed: {} ({})",
                    descriptor.name, d.code, d.message
                );
            }
        }
        Ok(tool_result(response))
    }

    fn invalid_tool(&self, d: &CommandDescriptor, message: String) -> ResponseEnvelope {
        ResponseEnvelope {
            schema_version: 1,
            session_id: self.executor.session_id().to_string(),
            request_id: None,
            command: d.name.to_string(),
            ok: false,
            replayed: false,
            result: None,
            diagnostics: vec![Diagnostic::error(
                Code::InvalidRequest,
                Stage::Request,
                message,
            )],
        }
    }

    fn run_tool(
        &mut self,
        d: &CommandDescriptor,
        mut args: Map<String, Value>,
    ) -> ResponseEnvelope {
        let mut document_id = None;
        if d.requires_document {
            match args.remove("documentId") {
                None | Some(Value::Null) => {}
                Some(Value::String(id)) => document_id = Some(id),
                Some(_) => return self.invalid_tool(d, "documentId must be a string".to_string()),
            }
            // A missing id means "the open document"; each connection has at most one.
            if document_id.is_none() {
                document_id = self.executor.current_document_id().map(str::to_owned);
            }
        }
        let mut expected_version = None;
        if d.requires_expected_version {
            match args.remove("expectedVersion") {
                None | Some(Value::Null) => {}
                Some(value) => match value.as_u64() {
                    Some(v) => expected_version = Some(v),
                    None => {
                        return self.invalid_tool(
                            d,
                            "expectedVersion must be a non-negative integer".to_string(),
                        )
                    }
                },
            }
        }
        let mut dry_run = false;
        if d.supports_dry_run {
            match args.remove("dryRun") {
                None | Some(Value::Null) => {}
                Some(Value::Bool(flag)) => dry_run = flag,
                Some(_) => return self.invalid_tool(d, "dryRun must be a boolean".to_string()),
            }
        }

        // Mirrors the executor's own rule for which requests take a number, so
        // the adapter's counter never drifts from the writer's.
        let sequence = (d.uses_request_sequence && !dry_run).then(|| {
            let n = self.next_seq;
            self.next_seq += 1;
            n
        });
        let request = RequestEnvelope {
            schema_version: 1,
            session_id: Some(self.executor.session_id().to_string()),
            writer_id: Some(self.writer_id.clone()),
            request_sequence: sequence,
            request_id: None,
            document_id,
            expected_version,
            command: d.name.to_string(),
            dry_run,
            params: Value::Object(args),
        };
        self.executor.execute(request)
    }
}

fn valid_id(id: &Value) -> bool {
    id.is_string()
        || id.as_number().is_some_and(|n| {
            let raw = n.to_string();
            let digits = raw.strip_prefix('-').unwrap_or(&raw);
            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
        })
}

fn guarded<T>(work: impl FnOnce() -> T) -> Result<T, RpcError> {
    catch_unwind(AssertUnwindSafe(work))
        .map_err(|_| RpcError::new(-32603, "internal request panic; restart server"))
}

fn server_info() -> Value {
    json!({"name":"hae-mcp","version":env!("CARGO_PKG_VERSION")})
}
fn capabilities() -> Value {
    json!({"tools":{"listChanged":false}})
}
fn initialize_result(version: &str) -> Value {
    json!({
        "protocolVersion": version,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": descriptions::server_instructions(),
    })
}

/// Build a tool's `inputSchema` from the registry schema plus the envelope
/// fields the agent controls (`documentId`, `expectedVersion`, `dryRun`).
pub fn input_schema(d: &CommandDescriptor) -> Value {
    let mut schema = d.input_schema.clone();
    if !schema.is_object() {
        schema = json!({ "type": "object" });
    }
    let root = schema.as_object_mut().expect("schema is an object");
    root.insert("type".into(), json!("object"));
    let mut extra: Vec<(&str, Value, bool)> = Vec::new();
    if d.requires_document {
        extra.push((
            "documentId",
            json!({ "type": "string", "description": descriptions::document_id_property(false) }),
            false,
        ));
    }
    if d.requires_expected_version {
        extra.push((
            "expectedVersion",
            json!({
                "type": "integer",
                "minimum": 0,
                "maximum": u64::MAX,
                "description": descriptions::expected_version_property(),
            }),
            true,
        ));
    }
    if d.supports_dry_run {
        extra.push((
            "dryRun",
            json!({
                "type": "boolean",
                "default": false,
                "description": descriptions::dry_run_property(),
            }),
            false,
        ));
    }
    let properties = root
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("properties is an object");
    for (name, property, _) in &extra {
        properties.insert((*name).to_string(), property.clone());
    }
    let required = root
        .entry("required")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("required is an array");
    for (name, _, is_required) in &extra {
        if *is_required && !required.iter().any(|r| r == name) {
            required.push(json!(name));
        }
    }
    if required.is_empty() {
        root.remove("required");
    }
    portable_schema(&mut schema);
    schema
}

fn rpc_error(id: &Value, error: &RpcError) -> Value {
    let mut result = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": error.code, "message": error.message },
    });
    if let Some(data) = &error.data {
        result["error"]["data"] = data.clone();
    }
    result
}

fn tool_result(mut response: ResponseEnvelope) -> Value {
    let mut text = serde_json::to_string(&response).unwrap_or_default();
    if text.len() > MAX_RESPONSE_BYTES {
        response = response_too_large(&response, text.len());
        text = serde_json::to_string(&response).unwrap_or_default();
    }
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": !response.ok,
        "structuredContent": serde_json::to_value(&response).unwrap_or_else(|_| json!({})),
    })
}

fn write_line<W: Write>(out: &mut W, value: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string(value).unwrap_or_default();
    if text.len() > MAX_RESPONSE_BYTES {
        // The MCP envelope escapes the JSON tool response a second time.
        // Enforce the limit on these final wire bytes, retaining the request id.
        let inner = value
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .and_then(|s| serde_json::from_str::<ResponseEnvelope>(s).ok());
        let mut limited = if let Some(response) = inner {
            json!({ "jsonrpc": "2.0", "id": value.get("id").unwrap_or(&Value::Null),
                "result": tool_result(response_too_large(&response, text.len())) })
        } else {
            rpc_error(
                value.get("id").unwrap_or(&Value::Null),
                &RpcError::new(
                    -32603,
                    "RESOURCE_LIMIT: JSON-RPC response exceeds the 4MiB wire limit",
                ),
            )
        };
        if value.pointer("/result/resultType").is_some() && limited.get("result").is_some() {
            limited["result"]["resultType"] = json!("complete");
            limited["result"]["_meta"] = value["result"]["_meta"].clone();
        }
        text = serde_json::to_string(&limited).unwrap_or_default();
        if text.len() > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bounded JSON-RPC response exceeds wire limit",
            ));
        }
    }
    out.write_all(text.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// Serve JSON-RPC messages until EOF.
pub fn serve<R: BufRead, W: Write>(
    server: &mut McpServer,
    reader: &mut R,
    out: &mut W,
) -> io::Result<()> {
    loop {
        let reply = match read_line_limited(reader, MAX_REQUEST_BYTES)? {
            Line::Eof => return Ok(()),
            Line::TooLong { bytes } => Some(rpc_error(
                &Value::Null,
                &RpcError::new(
                    INVALID_REQUEST,
                    format!("message of {bytes} bytes exceeds the {MAX_REQUEST_BYTES} byte limit"),
                ),
            )),
            Line::Text(bytes) => {
                if bytes.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                server.handle_message(&bytes)
            }
        };
        if let Some(reply) = reply {
            write_line(out, &reply)?;
        }
    }
}

#[cfg(test)]
mod panic_tests {
    use super::*;
    #[test]
    fn forced_unwind_becomes_internal_error() {
        let result = guarded(|| panic!("synthetic handler panic"));
        assert_eq!(result.unwrap_err().code, -32603);
        assert!(guarded(|| 42).is_ok());
    }
}
