//! Sandboxed PNG raster worker. Reads one staged SVG and font snapshot named
//! on the command line and writes a PNG and a JSON result to explicit paths.
//! It is started only by the preview adapter inside bubblewrap; see
//! `hae_render_rhwp::worker` for the protocol.

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(hae_render_rhwp::worker::run_cli(
        std::env::args_os().skip(1),
    ))
}
