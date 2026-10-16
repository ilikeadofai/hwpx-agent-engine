//! hae-mcp: stdio MCP server (JSON-RPC 2.0, one message per line) over the session executor.
//!
//! Every registry command becomes one MCP tool (`cell.setText` → `cell_set_text`).
//! The agent supplies `documentId`, `expectedVersion`, `dryRun` and the command
//! params only. The adapter issues one writer per connection and numbers
//! sequence-tracked requests itself (spec §6), so a duplicated agent call
//! fails with `VERSION_CONFLICT` instead of being applied twice.

pub mod descriptions;

use hae_diag::{Code, Diagnostic, EngineError, Stage};
use hae_exec::{
    command_list, CommandCategory, CommandDescriptor, RequestEnvelope, ResponseEnvelope,
    SessionExecutor,
};
use hae_jsonl::{encode_response, read_line_limited, Line, MAX_REQUEST_BYTES};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

/// Newest first. The first entry is offered when the client asks for an unknown version.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub struct McpServer {
    executor: SessionExecutor,
    writer_id: String,
    next_seq: u64,
    commands: Vec<CommandDescriptor>,
}

impl McpServer {
    pub fn new(mut executor: SessionExecutor) -> Result<Self, EngineError> {
        let writer_id = executor.issue_writer()?;
        Ok(Self {
            executor,
            writer_id,
            next_seq: 1,
            commands: command_list(),
        })
    }

    /// The `tools/list` result: one tool per registry command.
    pub fn tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .commands
            .iter()
            .map(|d| {
                let description = descriptions::description(d.name).unwrap_or(d.summary);
                let read_only = matches!(
                    d.category,
                    CommandCategory::Introspection | CommandCategory::Query
                );
                json!({
                    "name": d.mcp_tool_name,
                    "description": description,
                    "inputSchema": input_schema(d),
                    "annotations": { "readOnlyHint": read_only },
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
            // Notification: act on it if we care, never answer.
            eprintln!("hae-mcp: notification {method}");
            return None;
        };
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        let outcome = match method {
            "initialize" => Ok(initialize_result(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.tools_list()),
            "tools/call" => self.tools_call(&params),
            other => Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            )),
        };
        Some(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => rpc_error(&id, &e),
        })
    }

    fn tools_call(&mut self, params: &Value) -> Result<Value, RpcError> {
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
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(_) => {
                return Err(RpcError::new(
                    INVALID_PARAMS,
                    "`arguments` must be a JSON object",
                ))
            }
        };
        let response = self.run_tool(&descriptor, arguments);
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
        Ok(json!({
            "content": [{ "type": "text", "text": encode_response(&response) }],
            "isError": !response.ok,
        }))
    }

    fn run_tool(
        &mut self,
        d: &CommandDescriptor,
        mut args: Map<String, Value>,
    ) -> ResponseEnvelope {
        let invalid = |this: &Self, message: String| ResponseEnvelope {
            schema_version: 1,
            session_id: this.executor.session_id().to_string(),
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
        };

        let mut document_id = None;
        if d.requires_document {
            match args.remove("documentId") {
                None | Some(Value::Null) => {}
                Some(Value::String(id)) => document_id = Some(id),
                Some(_) => return invalid(self, "documentId must be a string".to_string()),
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
                        return invalid(
                            self,
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
                Some(_) => return invalid(self, "dryRun must be a boolean".to_string()),
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

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "hae-mcp", "version": env!("CARGO_PKG_VERSION") },
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
            json!({ "type": "string", "description": descriptions::document_id_property() }),
            true,
        ));
    }
    if d.requires_expected_version {
        extra.push((
            "expectedVersion",
            json!({
                "type": "integer",
                "minimum": 0,
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
    schema
}

fn rpc_error(id: &Value, error: &RpcError) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": error.code, "message": error.message },
    })
}

fn write_line<W: Write>(out: &mut W, value: &Value) -> io::Result<()> {
    let text = serde_json::to_string(value).unwrap_or_default();
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
