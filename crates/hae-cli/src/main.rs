//! hae: HWPX document engine CLI (plan 1-18).
//!
//! Two modes, both executing through [`hae_exec::SessionExecutor`] only:
//!
//! * `hae script` runs a JSON step list in a single session, so `history.undo`
//!   and `history.redo` are available. The protocol goes to stdout as a JSON
//!   array of `ResponseEnvelope`s; diagnostics go to stderr.
//! * `hae run` runs one command in an isolated one-shot session. Undo/Redo
//!   across independent invocations is rejected with `SESSION_REQUIRED`
//!   (spec §6).
//!
//! Script steps may reference earlier results: `$revision`, `$lastMatchNodeId`,
//! `$lastSnapshotId`, and named values captured with `"saveAs": "name"` (read
//! back as `$name` or `$name.path.to.field`).

use clap::{Args, Parser, Subcommand};
use hae_diag::{Code, Diagnostic, Limits, Stage};
use hae_exec::{
    command_describe, command_list, CommandCategory, ResponseEnvelope, SessionExecutor,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;

const EXIT_OK: i32 = 0;
const EXIT_FAILED: i32 = 1;
const EXIT_USAGE: i32 = 2;

/// Extra help line printed with `--help`: how `document.preview` gets its
/// sandboxed renderer.
const RENDERER_HELP: &str = "\
Preview environment (used by document.preview):
  HAE_RHWP      Path to the sandboxed rhwp renderer executable (enables preview)
  HAE_BWRAP     Path to bubblewrap (default /usr/bin/bwrap)
  HAE_FONT_DIR  Font directory mounted for rendering (default /usr/share/fonts)
  HAE_RASTER    PNG worker executable (default hae-raster next to this application)";

#[derive(Parser)]
#[command(
    name = "hae",
    version,
    about = "HWPX document engine CLI: one-session command scripts and one-shot commands",
    after_help = RENDERER_HELP
)]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Run a JSON step list in one session (Undo/Redo available)
    Script(ScriptArgs),
    /// Run one command in an isolated one-shot session
    Run(RunArgs),
}

#[derive(Args)]
#[command(after_help = RENDERER_HELP)]
struct ScriptArgs {
    /// Root directory for HWPX/image inputs (must already exist)
    #[arg(long, default_value = "/tmp")]
    input_root: PathBuf,
    /// Root directory for exports and project saves (must already exist)
    #[arg(long, default_value = "/tmp")]
    output_root: PathBuf,
    /// Step file, or '-' / omitted to read the script from stdin
    #[arg(value_name = "SCRIPT_FILE_OR_STDIN")]
    script: Option<PathBuf>,
}

#[derive(Args)]
#[command(after_help = RENDERER_HELP)]
struct RunArgs {
    /// Root directory for HWPX/image inputs (must already exist)
    #[arg(long, default_value = "/tmp")]
    input_root: PathBuf,
    /// Root directory for exports and project saves (must already exist)
    #[arg(long, default_value = "/tmp")]
    output_root: PathBuf,
    /// Registered command name, e.g. document.open
    #[arg(long)]
    command: String,
    /// JSON object with the command params
    #[arg(long)]
    params: Option<String>,
    /// expectedVersion for content mutations and Undo/Redo
    #[arg(long)]
    expected_version: Option<u64>,
    /// Compute the result for edit commands without applying it
    #[arg(long)]
    dry_run: bool,
}

fn main() {
    let exit = match Cli::parse().mode {
        Mode::Script(args) => run_script(args),
        Mode::Run(args) => run_once(args),
    };
    std::process::exit(exit);
}

/// Turn on `document.preview` when `HAE_RHWP` is set. A configuration failure
/// is reported once on stderr and the session keeps running with preview
/// unavailable; nothing is ever written to stdout.
fn configure_preview(executor: &mut SessionExecutor) {
    if std::env::var_os("HAE_RHWP")
        .filter(|v| !v.is_empty())
        .is_none()
    {
        return;
    }
    if !executor.configure_renderer_from_env() {
        eprintln!(
            "hae: preview renderer not configured (HAE_RHWP/HAE_BWRAP/HAE_FONT_DIR); \
             document.preview will return RENDERER_UNAVAILABLE"
        );
    }
}

/// One script step. Unknown JSON keys are ignored so scripts can carry
/// annotations; the params themselves are validated by the registry.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStep {
    command: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    expected_version: Option<Value>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    save_as: Option<String>,
}

fn run_script(args: ScriptArgs) -> i32 {
    let text = match read_script(args.script.as_ref()) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("hae: cannot read script input: {e}");
            return EXIT_USAGE;
        }
    };
    let steps = match parse_steps(&text) {
        Ok(steps) => steps,
        Err(e) => {
            eprintln!("hae: invalid script: {e}");
            return EXIT_USAGE;
        }
    };
    // Reject unregistered command strings up front (spec §6 / plan 1-17) so a
    // typo cannot leave partial sessions, files or locks behind.
    let registered: HashSet<&str> = command_list().iter().map(|c| c.name).collect();
    if let Some((index, step)) = steps
        .iter()
        .enumerate()
        .find(|(_, step)| !registered.contains(step.command.as_str()))
    {
        eprintln!(
            "hae: step {index}: unknown command '{}' (see `hae run --command command.list`)",
            step.command
        );
        return EXIT_USAGE;
    }

    let mut executor =
        match SessionExecutor::new(&args.input_root, &args.output_root, Limits::default()) {
            Ok(executor) => executor,
            Err(e) => {
                eprintln!("hae: cannot initialize session: {e}");
                return EXIT_USAGE;
            }
        };
    configure_preview(&mut executor);

    let mut vars = Vars::default();
    let mut responses: Vec<ResponseEnvelope> = Vec::with_capacity(steps.len());
    let mut failed = false;

    for (index, step) in steps.iter().enumerate() {
        let revision = executor.current_revision().unwrap_or(0);
        let params = match vars.resolve(&step.params, revision) {
            Ok(params) => params,
            Err(e) => {
                eprintln!("hae: step {index} ({}): {e}", step.command);
                return EXIT_USAGE;
            }
        };
        let expected = match resolve_expected_version(&step.expected_version, &vars, revision) {
            Ok(expected) => expected,
            Err(e) => {
                eprintln!("hae: step {index} ({}): {e}", step.command);
                return EXIT_USAGE;
            }
        };
        let response = executor.call(&step.command, params, expected, step.dry_run);
        log_step(index, &step.command, step.dry_run, &response);
        if response.ok {
            if let Some(result) = response.result.as_ref() {
                vars.observe(&step.command, result);
                if let Some(name) = &step.save_as {
                    vars.named.insert(name.clone(), result.clone());
                }
            }
        } else {
            failed = true;
        }
        responses.push(response);
    }

    match serde_json::to_string(&responses) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("hae: cannot serialize responses: {e}");
            return EXIT_USAGE;
        }
    }
    if failed {
        EXIT_FAILED
    } else {
        EXIT_OK
    }
}

fn run_once(args: RunArgs) -> i32 {
    let RunArgs {
        input_root,
        output_root,
        command,
        params,
        expected_version,
        dry_run,
    } = args;

    // Undo/Redo need the in-memory session; a one-shot process has none.
    if command_describe(&command).is_ok_and(|d| d.category == CommandCategory::History) {
        let response = session_required_response(&command);
        match serde_json::to_string(&response) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("hae: cannot serialize response: {e}");
                return EXIT_USAGE;
            }
        }
        eprintln!(
            "hae: '{command}' requires an open session; use `hae script` or a JSON-lines/MCP session (SESSION_REQUIRED)"
        );
        return EXIT_FAILED;
    }

    let params = match params {
        None => json!({}),
        Some(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(e) => {
                eprintln!("hae: invalid --params JSON: {e}");
                return EXIT_USAGE;
            }
        },
    };

    let mut executor = match SessionExecutor::new(&input_root, &output_root, Limits::default()) {
        Ok(executor) => executor,
        Err(e) => {
            eprintln!("hae: cannot initialize session: {e}");
            return EXIT_USAGE;
        }
    };
    configure_preview(&mut executor);
    let response = executor.call(&command, params, expected_version, dry_run);
    log_step(0, &command, dry_run, &response);
    match serde_json::to_string(&response) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("hae: cannot serialize response: {e}");
            return EXIT_USAGE;
        }
    }
    if response.ok {
        EXIT_OK
    } else {
        EXIT_FAILED
    }
}

fn session_required_response(command: &str) -> ResponseEnvelope {
    ResponseEnvelope {
        schema_version: 1,
        session_id: "ses-cli-oneshot".to_string(),
        request_id: None,
        command: command.to_string(),
        ok: false,
        replayed: false,
        result: None,
        diagnostics: vec![Diagnostic::error(
            Code::SessionRequired,
            Stage::Request,
            format!(
                "'{command}' requires an open session; Undo/Redo is not available between independent one-shot invocations"
            ),
        )],
    }
}

fn log_step(index: usize, command: &str, dry_run: bool, response: &ResponseEnvelope) {
    if response.ok {
        eprintln!("hae: step {index} {command} ok");
    } else {
        for diagnostic in &response.diagnostics {
            eprintln!(
                "hae: step {index} {command} failed: {} ({})",
                diagnostic.code, diagnostic.message
            );
        }
        if response.diagnostics.is_empty() {
            eprintln!("hae: step {index} {command} failed");
        }
    }
    if dry_run {
        eprintln!("hae: step {index} {command} was a dry run");
    }
}

fn read_script(path: Option<&PathBuf>) -> std::io::Result<String> {
    match path {
        Some(p) if p.as_os_str() != "-" => std::fs::read_to_string(p),
        _ => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            Ok(text)
        }
    }
}

/// Parse a JSON array of steps, a single JSON object, or newline-delimited
/// JSON objects. Blank lines and `#` comments are skipped in NDJSON mode.
fn parse_steps(text: &str) -> Result<Vec<RawStep>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(steps) = serde_json::from_str::<Vec<RawStep>>(trimmed) {
        return Ok(steps);
    }
    if let Ok(step) = serde_json::from_str::<RawStep>(trimmed) {
        return Ok(vec![step]);
    }
    let mut steps = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let step: RawStep =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", line_number + 1))?;
        steps.push(step);
    }
    if steps.is_empty() {
        return Err("script has no steps".to_string());
    }
    Ok(steps)
}

fn resolve_expected_version(
    raw: &Option<Value>,
    vars: &Vars,
    revision: u64,
) -> Result<Option<u64>, String> {
    let Some(value) = raw else {
        return Ok(None);
    };
    match vars.resolve(value, revision)? {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("expectedVersion must be a non-negative integer, got {n}")),
        Value::String(s) => s
            .parse::<u64>()
            .map(Some)
            .map_err(|_| format!("expectedVersion must be an integer, got '{s}'")),
        other => Err(format!("expectedVersion must be an integer, got {other}")),
    }
}

/// Values captured from earlier responses plus the dynamic session revision.
#[derive(Default)]
struct Vars {
    last_match_node_id: Option<String>,
    last_snapshot_id: Option<String>,
    named: HashMap<String, Value>,
}

impl Vars {
    fn observe(&mut self, command: &str, result: &Value) {
        match command {
            "element.query" => {
                if let Some(id) = first_match_node_id(result) {
                    self.last_match_node_id = Some(id);
                }
            }
            "document.snapshot" => {
                if let Some(id) = result.get("snapshotId").and_then(Value::as_str) {
                    self.last_snapshot_id = Some(id.to_string());
                }
            }
            _ => {}
        }
    }

    /// Deep-substitute every `$reference` inside a JSON value.
    fn resolve(&self, value: &Value, revision: u64) -> Result<Value, String> {
        match value {
            Value::Array(items) => {
                let mut resolved = Vec::with_capacity(items.len());
                for item in items {
                    resolved.push(self.resolve(item, revision)?);
                }
                Ok(Value::Array(resolved))
            }
            Value::Object(map) => {
                let mut resolved = Map::new();
                for (key, item) in map {
                    resolved.insert(key.clone(), self.resolve(item, revision)?);
                }
                Ok(Value::Object(resolved))
            }
            Value::String(s) => self.resolve_string(s, revision),
            other => Ok(other.clone()),
        }
    }

    fn resolve_string(&self, source: &str, revision: u64) -> Result<Value, String> {
        // A string that is exactly one reference keeps the referenced JSON type
        // (so "$revision" becomes a number, not the text "3").
        if let Some(value) = self.whole_reference(source, revision)? {
            return Ok(value);
        }
        if !source.contains('$') {
            return Ok(Value::String(source.to_string()));
        }
        let mut rendered = String::with_capacity(source.len());
        let mut cursor = source;
        while let Some(at) = cursor.find('$') {
            rendered.push_str(&cursor[..at]);
            let rest = &cursor[at + 1..];
            if let Some((value, consumed)) = self.parse_reference(rest, revision)? {
                rendered.push_str(&stringify(&value));
                cursor = &rest[consumed..];
            } else {
                rendered.push('$');
                cursor = rest;
            }
        }
        rendered.push_str(cursor);
        Ok(Value::String(rendered))
    }

    fn whole_reference(&self, source: &str, revision: u64) -> Result<Option<Value>, String> {
        let Some(rest) = source.strip_prefix('$') else {
            return Ok(None);
        };
        let Some((value, consumed)) = self.parse_reference(rest, revision)? else {
            return Ok(None);
        };
        Ok((consumed == rest.len()).then_some(value))
    }

    /// Parse one reference after `$`. `$name`, `$name.field`, `$name.0.field`
    /// and `${name.field}` are accepted. Returns the value and bytes consumed.
    fn parse_reference(
        &self,
        input: &str,
        revision: u64,
    ) -> Result<Option<(Value, usize)>, String> {
        let braced = input.starts_with('{');
        let (raw_name, mut consumed) = if braced {
            let inner = &input[1..];
            let Some(end) = inner.find('}') else {
                return Err("unterminated ${...} reference in script".to_string());
            };
            (inner[..end].to_string(), end + 2)
        } else {
            let len = identifier_len(input);
            if len == 0 {
                return Ok(None);
            }
            (input[..len].to_string(), len)
        };

        let mut segments: Vec<String> = raw_name.split('.').map(str::to_string).collect();
        let base = segments.remove(0);
        if !braced {
            loop {
                let remaining = &input[consumed..];
                let Some(after_dot) = remaining.strip_prefix('.') else {
                    break;
                };
                let len = identifier_len(after_dot);
                if len == 0 {
                    break;
                }
                segments.push(after_dot[..len].to_string());
                consumed += 1 + len;
            }
        }

        let mut value = self.base_value(&base, revision)?;
        for segment in &segments {
            value = index_value(value, segment)?;
        }
        Ok(Some((value, consumed)))
    }

    fn base_value(&self, name: &str, revision: u64) -> Result<Value, String> {
        match name {
            "revision" => Ok(Value::from(revision)),
            "lastMatchNodeId" | "lastMatchId" => self
                .last_match_node_id
                .clone()
                .map(Value::String)
                .ok_or_else(|| {
                    "variable $lastMatchNodeId is unset (run element.query first)".to_string()
                }),
            "lastSnapshotId" => self
                .last_snapshot_id
                .clone()
                .map(Value::String)
                .ok_or_else(|| {
                    "variable $lastSnapshotId is unset (run document.snapshot first)".to_string()
                }),
            other => self
                .named
                .get(other)
                .cloned()
                .ok_or_else(|| format!("unknown script variable: ${other}")),
        }
    }
}

fn identifier_len(input: &str) -> usize {
    input
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count()
}

fn index_value(value: Value, segment: &str) -> Result<Value, String> {
    match value {
        Value::Object(map) => map
            .get(segment)
            .cloned()
            .ok_or_else(|| format!("script reference has no field '{segment}'")),
        Value::Array(items) => {
            let index: usize = segment
                .parse()
                .map_err(|_| format!("array reference segment '{segment}' is not an index"))?;
            items
                .get(index)
                .cloned()
                .ok_or_else(|| format!("script reference index {index} is out of range"))
        }
        other => Err(format!("cannot look up '{segment}' on {other}")),
    }
}

fn first_match_node_id(result: &Value) -> Option<String> {
    result
        .get("matches")?
        .as_array()?
        .first()?
        .get("nodeId")?
        .as_str()
        .map(str::to_string)
}

fn stringify(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}
