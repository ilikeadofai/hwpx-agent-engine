//! `hae-mcp`: stdio MCP server (plan 2-2). Protocol on stdout, logs on stderr.

use clap::Parser;
use hae_diag::Limits;
use hae_exec::SessionExecutor;
use hae_mcp::McpServer;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "hae-mcp",
    version,
    about = "HWPX document engine MCP server (stdio, JSON-RPC 2.0, one message per line)"
)]
struct Cli {
    /// Root directory for HWPX/image inputs (must already exist)
    #[arg(long, default_value = "/tmp")]
    input_root: PathBuf,
    /// Root directory for exports and project saves (must already exist)
    #[arg(long, default_value = "/tmp")]
    output_root: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    let executor = match SessionExecutor::new(&cli.input_root, &cli.output_root, Limits::default())
    {
        Ok(executor) => executor,
        Err(e) => {
            eprintln!("hae-mcp: cannot initialize session: {e}");
            std::process::exit(2);
        }
    };
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
