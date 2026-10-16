//! `hae-jsonl`: JSON-lines server on stdin/stdout (plan 2-1).

use clap::Parser;
use hae_diag::Limits;
use hae_exec::SessionExecutor;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "hae-jsonl",
    version,
    about = "HWPX document engine JSON-lines server: one request per line, one response per line",
    after_help = RENDERER_HELP
)]
struct Cli {
    /// Root directory for HWPX/image inputs (must already exist)
    #[arg(long, default_value = "/tmp")]
    input_root: PathBuf,
    /// Root directory for exports and project saves (must already exist)
    #[arg(long, default_value = "/tmp")]
    output_root: PathBuf,
}

/// Extra help line printed with `--help`: how `document.preview` gets its
/// sandboxed renderer.
const RENDERER_HELP: &str = "\
Preview environment (used by document.preview):
  HAE_RHWP      Path to the sandboxed rhwp renderer executable (enables preview)
  HAE_BWRAP     Path to bubblewrap (default /usr/bin/bwrap)
  HAE_FONT_DIR  Font directory mounted for rendering (default /usr/share/fonts)
  HAE_RASTER    PNG worker executable (default hae-raster next to this application)";

fn main() {
    let cli = Cli::parse();
    let mut executor =
        match SessionExecutor::new(&cli.input_root, &cli.output_root, Limits::default()) {
            Ok(executor) => executor,
            Err(e) => {
                eprintln!("hae-jsonl: cannot initialize session: {e}");
                std::process::exit(2);
            }
        };
    // Enable document.preview when HAE_RHWP is set. On failure log once to
    // stderr (never stdout) and keep serving with preview unavailable.
    if std::env::var_os("HAE_RHWP")
        .filter(|v| !v.is_empty())
        .is_some()
        && !executor.configure_renderer_from_env()
    {
        eprintln!(
            "hae-jsonl: preview renderer not configured (HAE_RHWP/HAE_BWRAP/HAE_FONT_DIR); \
             document.preview will return RENDERER_UNAVAILABLE"
        );
    }
    eprintln!("hae-jsonl: session {} ready", executor.session_id());
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    if let Err(e) = hae_jsonl::serve(&mut executor, &mut reader, &mut out) {
        eprintln!("hae-jsonl: transport error: {e}");
        std::process::exit(1);
    }
}
