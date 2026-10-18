//! `hae-mcp`: stdio MCP server (plan 2-2). Protocol on stdout, logs on stderr.

use clap::{Parser, Subcommand};
use hae_diag::Limits;
use hae_exec::SessionExecutor;
use hae_mcp::McpServer;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "hae-mcp",
    version,
    about = "HWPX document engine MCP server (stdio, JSON-RPC 2.0, one message per line)",
    after_help = RENDERER_HELP
)]
struct Cli {
    /// Root directory for HWPX/image inputs (must already exist)
    #[arg(long, global = true, default_value = "/tmp")]
    input_root: PathBuf,
    /// Root directory for exports and project saves (must already exist)
    #[arg(long, global = true, default_value = "/tmp")]
    output_root: PathBuf,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Check preview executables, font directory and input/output roots; print JSON
    Doctor,
}

/// Extra help line printed with `--help`: how `document.preview` gets its
/// sandboxed renderer. The text lives with the discovery rules.
const RENDERER_HELP: &str = hae_exec::RENDERER_ENV_HELP;

fn main() {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Action::Doctor)) {
        let report = hae_mcp::doctor::report(&cli.input_root, &cli.output_root);
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("doctor JSON")
        );
        std::process::exit(if report["ok"] == true { 0 } else { 1 });
    }
    let mut executor =
        match SessionExecutor::new(&cli.input_root, &cli.output_root, Limits::default()) {
            Ok(executor) => executor,
            Err(e) => {
                eprintln!("hae-mcp: cannot initialize session: {e}");
                std::process::exit(2);
            }
        };
    // Enable document.preview: explicit HAE_* values first, then the
    // documented search order. On failure log once to stderr (never stdout)
    // and keep serving with preview unavailable.
    if !executor.configure_renderer_from_env() {
        match executor.renderer_configuration_note() {
            Some(note) => eprintln!("hae-mcp: preview renderer not configured: {note}"),
            None => eprintln!(
                "hae-mcp: preview renderer not configured (HAE_RHWP/HAE_BWRAP/HAE_FONT_DIR); \
                 document.preview will return RENDERER_UNAVAILABLE"
            ),
        }
    } else if let Some(note) = executor.renderer_platform_note() {
        // Configured, but this platform cannot isolate the renderer.
        eprintln!("hae-mcp: preview is unavailable on this platform: {note}");
    }
    let mut server = match McpServer::new(executor) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("hae-mcp: cannot issue writer: {e}");
            std::process::exit(2);
        }
    };
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    if let Err(e) = hae_mcp::serve(&mut server, &mut reader, &mut out) {
        eprintln!("hae-mcp: transport error: {e}");
        std::process::exit(1);
    }
}
