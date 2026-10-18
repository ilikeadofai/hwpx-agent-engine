//! Supervised PNG raster worker (spec §9·§10, residual P1 of the resumption
//! review).
//!
//! resvg parses SVG, builds paths/filters, loads fonts and encodes PNG with
//! allocations no in-process byte reservation can bound. Product preview
//! therefore rasterises only in a separate `hae-raster` process that runs
//! inside the same bubblewrap sandbox as rhwp (network off, read-only `/usr`,
//! read-only `/in`, writable `/out` only) behind the same trusted
//! `prlimit --as` wrapper. There is no in-process fallback: a missing or
//! failing worker is an explicit error or PNG failure, never a host render.
//!
//! Protocol (`hae-raster-v1`). The parent stages, read-only, a copy of the
//! worker executable (hashed), the page SVG, and an exact snapshot of the
//! already hashed font payloads under generated numeric filenames plus a
//! manifest. The worker re-verifies every length and SHA-256, rasterises with
//! [`crate::png::rasterize_svg_with_budget`] and writes only `/out/page.png`
//! and, last, `/out/result.json`. The parent trusts none of it: it inventories
//! `/out`, checks regular files, exact sizes, PNG signature/IHDR/IEND, pixel
//! caps and the SHA-256 it computes itself.

#![cfg_attr(not(unix), allow(dead_code))]

use crate::fonts::{FontSet, SnapshotEntry};
use crate::png::{rasterize_svg_with_budget, CheckedSvg, PngOptions};
use crate::sandbox::{CancelToken, RunError, RunLimits, Sandbox, TempDir};
use crate::{read_out_file_bounded, unavailable};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::sha256_hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn create_dir_with_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::DirBuilder::new().mode(mode).create(path)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        fs::create_dir(path)
    }
}

fn create_file_with_mode(path: &Path, mode: u32) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(mode);
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

fn set_file_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "renderer worker staging requires Unix executable permissions",
        ))
    }
}

/// Executable name looked up next to the running application by default.
pub const WORKER_NAME: &str = "hae-raster";
/// Protocol identifier recorded in worker results and preview metadata.
pub const PROTOCOL: &str = "hae-raster-v1";
/// Smallest `RLIMIT_AS` the worker is started with. Measured on a debug build:
/// an 800x1100 text page aborts at 32 MiB and renders at 48 MiB, so the
/// binary, libc, fonts and one page already map most of that. Below this
/// floor a start would fail for reasons unrelated to the document.
pub(crate) const MIN_ADDRESS_SPACE: u64 = 64 * 1024 * 1024;
/// Parent-side workspace reserved for the bounded `result.json` read/parse.
pub(crate) const RESULT_WORKSPACE: u64 = 8 * 1024 * 1024;

const MAX_WORKER_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SVG_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RESULT_BYTES: u64 = 256 * 1024;
const MAX_FAMILIES: usize = 1000;
const MAX_FAMILY_BYTES: usize = 200;
const SVG_NAME: &str = "page.svg";
const FONTS_DIR: &str = "fonts";
const MANIFEST_NAME: &str = "fonts.json";
const PNG_NAME: &str = "page.png";
const RESULT_NAME: &str = "result.json";
const IEND: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82];

// ---------------------------------------------------------------- protocol

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FontManifest {
    fonts: Vec<SnapshotEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PngInfo {
    file: String,
    bytes: u64,
    sha256: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerResult {
    protocol: String,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worker_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    svg_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fonts_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    png: Option<PngInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    font_families: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

// ------------------------------------------------------------ worker side

struct Args {
    svg: PathBuf,
    svg_bytes: u64,
    svg_sha256: String,
    fonts_dir: PathBuf,
    fonts_manifest: PathBuf,
    fonts_hash: String,
    png: PathBuf,
    result: PathBuf,
    scale: f32,
    max_pixels: u64,
    max_decoded_bytes: u64,
    max_png_bytes: u64,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> std::result::Result<Args, String> {
    let mut it = args.into_iter();
    let mut svg = None;
    let mut svg_bytes = None;
    let mut svg_sha256 = None;
    let mut fonts_dir = None;
    let mut fonts_manifest = None;
    let mut fonts_hash = None;
    let mut png = None;
    let mut result = None;
    let mut scale = None;
    let mut max_pixels = None;
    let mut max_decoded_bytes = None;
    let mut max_png_bytes = None;
    while let Some(flag) = it.next() {
        let flag = flag.to_str().ok_or("non-UTF-8 argument")?.to_string();
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let text = || value.to_str().map(str::to_string).ok_or("non-UTF-8 value");
        let number = |t: String| {
            t.parse::<u64>()
                .map_err(|_| format!("{flag}: not a number"))
        };
        match flag.as_str() {
            "--svg" => svg = Some(PathBuf::from(&value)),
            "--svg-bytes" => svg_bytes = Some(number(text()?)?),
            "--svg-sha256" => svg_sha256 = Some(text()?),
            "--fonts-dir" => fonts_dir = Some(PathBuf::from(&value)),
            "--fonts-manifest" => fonts_manifest = Some(PathBuf::from(&value)),
            "--fonts-hash" => fonts_hash = Some(text()?),
            "--png" => png = Some(PathBuf::from(&value)),
            "--result" => result = Some(PathBuf::from(&value)),
            "--scale" => {
                scale = Some(
                    text()?
                        .parse::<f32>()
                        .map_err(|_| format!("{flag}: not a number"))?,
                )
            }
            "--max-pixels" => max_pixels = Some(number(text()?)?),
            "--max-decoded-bytes" => max_decoded_bytes = Some(number(text()?)?),
            "--max-png-bytes" => max_png_bytes = Some(number(text()?)?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let need = |name: &str| format!("missing {name}");
    Ok(Args {
        svg: svg.ok_or_else(|| need("--svg"))?,
        svg_bytes: svg_bytes.ok_or_else(|| need("--svg-bytes"))?,
        svg_sha256: svg_sha256.ok_or_else(|| need("--svg-sha256"))?,
        fonts_dir: fonts_dir.ok_or_else(|| need("--fonts-dir"))?,
        fonts_manifest: fonts_manifest.ok_or_else(|| need("--fonts-manifest"))?,
        fonts_hash: fonts_hash.ok_or_else(|| need("--fonts-hash"))?,
        png: png.ok_or_else(|| need("--png"))?,
        result: result.ok_or_else(|| need("--result"))?,
        scale: scale.ok_or_else(|| need("--scale"))?,
        max_pixels: max_pixels.ok_or_else(|| need("--max-pixels"))?,
        max_decoded_bytes: max_decoded_bytes.ok_or_else(|| need("--max-decoded-bytes"))?,
        max_png_bytes: max_png_bytes.ok_or_else(|| need("--max-png-bytes"))?,
    })
}

/// A failure the worker reports in `result.json` (exit status 3).
struct Fail {
    kind: &'static str,
    message: String,
}

fn fail(kind: &'static str, message: impl Into<String>) -> Fail {
    Fail {
        kind,
        message: message.into(),
    }
}

fn read_exact_file(
    path: &Path,
    expected: u64,
    what: &'static str,
) -> std::result::Result<Vec<u8>, Fail> {
    let md =
        fs::symlink_metadata(path).map_err(|e| fail("input_mismatch", format!("{what}: {e}")))?;
    if !md.is_file() || md.len() != expected {
        return Err(fail(
            "input_mismatch",
            format!("{what}: not a regular file of {expected} bytes"),
        ));
    }
    let mut buf = Vec::new();
    buf.try_reserve_exact(expected as usize + 1)
        .map_err(|_| fail("memory_limit", format!("{what}: buffer allocation failed")))?;
    fs::File::open(path)
        .and_then(|f| f.take(expected + 1).read_to_end(&mut buf))
        .map_err(|e| fail("input_mismatch", format!("{what}: {e}")))?;
    if buf.len() as u64 != expected {
        return Err(fail(
            "input_mismatch",
            format!("{what}: size changed during read"),
        ));
    }
    Ok(buf)
}

fn rasterize_job(a: &Args) -> std::result::Result<WorkerResult, Fail> {
    if a.svg_bytes > MAX_SVG_BYTES {
        return Err(fail("input_limit", "SVG exceeds the worker input limit"));
    }
    let svg = read_exact_file(&a.svg, a.svg_bytes, "svg")?;
    if sha256_hex(&svg) != a.svg_sha256 {
        return Err(fail(
            "input_mismatch",
            "SVG differs from its declared SHA-256",
        ));
    }
    let manifest_len = fs::symlink_metadata(&a.fonts_manifest)
        .map_err(|e| fail("input_mismatch", format!("font manifest: {e}")))?
        .len();
    if manifest_len > MAX_MANIFEST_BYTES {
        return Err(fail("input_limit", "font manifest exceeds the limit"));
    }
    let manifest = read_exact_file(&a.fonts_manifest, manifest_len, "font manifest")?;
    let manifest: FontManifest = serde_json::from_slice(&manifest)
        .map_err(|e| fail("input_mismatch", format!("font manifest: {e}")))?;
    // The pre-checked decoded-buffer budget is a quarter of the address space
    // the supervisor granted; fonts are bounded by the same allowance.
    let fonts = FontSet::from_snapshot(
        &a.fonts_dir,
        &manifest.fonts,
        &a.fonts_hash,
        a.max_decoded_bytes.saturating_mul(4),
    )
    .map_err(|e| fail("font_snapshot", e.to_string()))?;
    let checked = CheckedSvg::new(&svg).map_err(|c| {
        fail(
            "svg_unsafe",
            format!("SVG failed the safety scan ({} issues)", c.issues.len()),
        )
    })?;
    let opts = PngOptions {
        scale: a.scale,
        max_pixels: a.max_pixels,
    };
    let png = rasterize_svg_with_budget(checked, &fonts, &opts, a.max_decoded_bytes)
        .map_err(|e| fail(error_kind(&e), e.to_string()))?;
    if png.len() as u64 > a.max_png_bytes {
        return Err(fail(
            "png_too_large",
            format!(
                "encoded PNG is {} bytes; the cap is {}",
                png.len(),
                a.max_png_bytes
            ),
        ));
    }
    if png.len() < 24 {
        return Err(fail("encode", "encoder produced a truncated PNG"));
    }
    let (width, height) =
        png_dimensions(&png).ok_or_else(|| fail("encode", "encoder produced no IHDR"))?;
    create_file_with_mode(&a.png, 0o644)
        .and_then(|mut f| f.write_all(&png))
        .map_err(|e| fail("output", format!("cannot write PNG: {e}")))?;
    let mut families = BTreeSet::new();
    let mut budget = MAX_RESULT_BYTES / 2;
    for family in fonts.families() {
        if family.len() <= MAX_FAMILY_BYTES
            && families.len() < MAX_FAMILIES
            && budget >= family.len() as u64 + 4
        {
            budget -= family.len() as u64 + 4;
            families.insert(family);
        }
    }
    Ok(WorkerResult {
        protocol: PROTOCOL.into(),
        status: "ok".into(),
        worker_version: Some(env!("CARGO_PKG_VERSION").into()),
        svg_sha256: Some(a.svg_sha256.clone()),
        fonts_hash: Some(fonts.hash.clone()),
        png: Some(PngInfo {
            file: PNG_NAME.into(),
            bytes: png.len() as u64,
            sha256: sha256_hex(&png),
            width,
            height,
        }),
        font_families: families.into_iter().collect(),
        kind: None,
        message: None,
    })
}

fn error_kind(e: &crate::png::PngError) -> &'static str {
    use crate::png::PngError::*;
    match e {
        Parse(_) => "parse",
        TooLarge { .. } => "too_large",
        Encode(_) => "encode",
        PixelLimit { .. } => "pixel_limit",
        MemoryLimit { .. } => "memory_limit",
        InvalidImage(_) => "invalid_image",
        UnsupportedImage(_) => "unsupported_image",
    }
}

fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 33 || png[..8] != *b"\x89PNG\r\n\x1a\n" || png[8..16] != *b"\0\0\0\x0dIHDR" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Entry point of the `hae-raster` executable. Returns its exit status:
/// 0 rendered, 2 usage error, 3 reported failure (`result.json` explains).
#[cfg(not(unix))]
pub fn run_cli(_args: impl IntoIterator<Item = OsString>) -> u8 {
    eprintln!(
        "hae-raster: renderer isolation is unavailable on {}; refusing to rasterize",
        std::env::consts::OS
    );
    2
}

#[cfg(unix)]
pub fn run_cli(args: impl IntoIterator<Item = OsString>) -> u8 {
    let a = match parse_args(args) {
        Ok(a) => a,
        Err(m) => {
            eprintln!("hae-raster: {m}");
            return 2;
        }
    };
    let result = match rasterize_job(&a) {
        Ok(r) => r,
        Err(f) => {
            eprintln!("hae-raster: {}: {}", f.kind, f.message);
            let mut message = f.message;
            message.truncate(message.floor_char_boundary(2048));
            WorkerResult {
                protocol: PROTOCOL.into(),
                status: "error".into(),
                worker_version: Some(env!("CARGO_PKG_VERSION").into()),
                svg_sha256: None,
                fonts_hash: None,
                png: None,
                font_families: Vec::new(),
                kind: Some(f.kind.into()),
                message: Some(message),
            }
        }
    };
    let ok = result.status == "ok";
    let written = serde_json::to_vec(&result)
        .map_err(std::io::Error::other)
        .and_then(|json| create_file_with_mode(&a.result, 0o644)?.write_all(&json));
    match (written, ok) {
        (Ok(()), true) => 0,
        (Ok(()), false) => 3,
        (Err(e), _) => {
            eprintln!("hae-raster: cannot write result: {e}");
            3
        }
    }
}

// ------------------------------------------------------------ parent side

/// Default worker location: `hae-raster` next to the running executable
/// (test executables in `target/<profile>/deps` map to `target/<profile>`).
/// Empty when the executable path is unknown, which is reported as
/// `raster_worker_not_configured` at preview time.
pub fn default_worker_path() -> PathBuf {
    let Some(mut dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return PathBuf::new();
    };
    if dir.file_name().is_some_and(|n| n == "deps") {
        dir.pop();
    }
    dir.join(WORKER_NAME)
}

/// Successfully validated worker output.
pub(crate) struct RasterPng {
    pub png: Vec<u8>,
    pub sha256: String,
    pub families: BTreeSet<String>,
}

/// Per-page result. Fatal conditions (cancellation, the overall deadline,
/// an unstartable sandbox) are `Err`; everything the worker itself can do
/// wrong is a contained per-page failure.
pub(crate) enum RasterOutcome {
    Png(RasterPng),
    Failed {
        reason: &'static str,
        message: String,
    },
}

pub(crate) struct PageJob<'a> {
    pub svg: &'a [u8],
    pub svg_sha256: &'a str,
    pub scale: f32,
    pub max_pixels: u64,
    pub max_decoded_bytes: u64,
    pub max_png_bytes: u64,
    /// `RLIMIT_AS` of the worker (per-process virtual address space).
    pub address_space: u64,
}

/// Read-only worker stage shared by all pages of one preview.
pub(crate) struct RasterStage {
    _td: TempDir,
    in_dir: PathBuf,
    out_root: PathBuf,
    sandbox: Sandbox,
    pub worker_sha256: String,
    pub worker_bytes: u64,
    pub fonts_hash: String,
    pub font_files: usize,
    pages: usize,
}

fn io_err(what: &str, e: std::io::Error) -> EngineError {
    EngineError::new(Code::IoError, Stage::Render, format!("{what}: {e}"))
}

impl RasterStage {
    /// Copy and hash the worker, write the exact font snapshot. Nothing from
    /// the live font directory is read here: the payloads were hashed when the
    /// set was scanned.
    pub(crate) fn prepare(
        worker: &Path,
        bwrap: &Path,
        work_dir: &Path,
        fonts: &FontSet,
    ) -> Result<RasterStage> {
        if worker.as_os_str().is_empty() {
            return Err(unavailable(
                "raster_worker_not_configured",
                "no raster worker configured: build `cargo build -p hae-render-rhwp --bin \
                 hae-raster` and place `hae-raster` beside this application, or set HAE_RASTER to \
                 its path",
            ));
        }
        let missing = |e: std::io::Error| {
            unavailable(
                "raster_worker_missing",
                format!(
                    "raster worker {}: {e}; set HAE_RASTER to an existing executable or build it \
                     beside this application (`cargo build -p hae-render-rhwp --bin hae-raster`)",
                    worker.display()
                ),
            )
        };
        let mut src = fs::File::open(worker).map_err(missing)?;
        let md = src.metadata().map_err(missing)?;
        if !md.is_file() || md.len() > MAX_WORKER_BYTES {
            return Err(unavailable(
                "raster_worker_missing",
                format!(
                    "raster worker {} is not a regular file within {MAX_WORKER_BYTES} bytes",
                    worker.display()
                ),
            ));
        }
        let td = TempDir::new_in(work_dir, "hae-raster-").map_err(|e| io_err("work dir", e))?;
        let mkdir = |p: &Path| create_dir_with_mode(p, 0o700).map_err(|e| io_err("mkdir", e));
        let in_dir = td.path().join("in");
        let out_root = td.path().join("out");
        // The sandbox always mounts a font directory at /fonts. Give it an
        // empty one so the live host font directory is not visible at all.
        let void = td.path().join("no-fonts");
        for d in [&in_dir, &out_root, &void, &in_dir.join(FONTS_DIR)] {
            mkdir(d)?;
        }
        let bin = in_dir.join(WORKER_NAME);
        let mut dst = create_file_with_mode(&bin, 0o755).map_err(|e| io_err("create worker", e))?;
        let mut hash = Sha256::new();
        let mut chunk = [0u8; 64 * 1024];
        let mut copied = 0u64;
        loop {
            let n = src.read(&mut chunk).map_err(missing)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            if copied > MAX_WORKER_BYTES {
                return Err(unavailable(
                    "raster_worker_missing",
                    "raster worker grew while copied",
                ));
            }
            dst.write_all(&chunk[..n])
                .map_err(|e| io_err("copy worker", e))?;
            hash.update(&chunk[..n]);
        }
        drop(dst);
        set_file_mode(&bin, 0o755).map_err(|e| io_err("chmod", e))?;
        let entries = fonts
            .write_snapshot(&in_dir.join(FONTS_DIR))
            .map_err(|e| io_err("font snapshot", e))?;
        let manifest = serde_json::to_vec(&FontManifest { fonts: entries })
            .map_err(|e| io_err("font manifest", std::io::Error::other(e)))?;
        fs::write(in_dir.join(MANIFEST_NAME), manifest).map_err(|e| io_err("font manifest", e))?;
        Ok(RasterStage {
            sandbox: Sandbox::new(bwrap, void),
            in_dir,
            out_root,
            worker_sha256: format!("{:x}", hash.finalize()),
            worker_bytes: copied,
            fonts_hash: fonts.hash.clone(),
            font_files: fonts.files.len(),
            pages: 0,
            _td: td,
        })
    }

    /// Run the worker for one page inside the sandbox.
    pub(crate) fn rasterize(
        &mut self,
        job: &PageJob<'_>,
        cancel: Option<&CancelToken>,
        deadline: Instant,
    ) -> Result<RasterOutcome> {
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(crate::run_error(RunError::Timeout(timeout)));
        }
        self.pages += 1;
        let out_dir = self.out_root.join(format!("p{:04}", self.pages));
        create_dir_with_mode(&out_dir, 0o700).map_err(|e| io_err("mkdir", e))?;
        let svg_path = self.in_dir.join(SVG_NAME);
        match fs::remove_file(&svg_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err("replace page svg", e)),
        }
        fs::write(&svg_path, job.svg).map_err(|e| io_err("write page svg", e))?;

        let run_limits = RunLimits {
            timeout,
            stdout_cap: 4096,
            stderr_cap: 16 * 1024,
            out_cap_bytes: job.max_png_bytes.saturating_add(2 * MAX_RESULT_BYTES),
            out_cap_entries: 8,
        };
        let mut argv: Vec<String> = vec![format!("/in/{WORKER_NAME}")];
        for (flag, value) in [
            ("--svg", format!("/in/{SVG_NAME}")),
            ("--svg-bytes", job.svg.len().to_string()),
            ("--svg-sha256", job.svg_sha256.to_string()),
            ("--fonts-dir", format!("/in/{FONTS_DIR}")),
            ("--fonts-manifest", format!("/in/{MANIFEST_NAME}")),
            ("--fonts-hash", self.fonts_hash.clone()),
            ("--png", format!("/out/{PNG_NAME}")),
            ("--result", format!("/out/{RESULT_NAME}")),
            ("--scale", job.scale.to_string()),
            ("--max-pixels", job.max_pixels.to_string()),
            ("--max-decoded-bytes", job.max_decoded_bytes.to_string()),
            ("--max-png-bytes", job.max_png_bytes.to_string()),
        ] {
            argv.push(flag.to_string());
            argv.push(value);
        }
        let argv_ref: Vec<&str> = argv.iter().map(String::as_str).collect();
        let out = match self.sandbox.run_with_memory(
            &self.in_dir,
            &out_dir,
            &argv_ref,
            &run_limits,
            cancel,
            Some(job.address_space),
        ) {
            Ok(o) => o,
            Err(
                e
                @ (RunError::StdoutLimit(_) | RunError::StderrLimit(_) | RunError::OutputLimit(_)),
            ) => {
                return Ok(RasterOutcome::Failed {
                    reason: "output_limit",
                    message: e.to_string(),
                })
            }
            Err(e) => return Err(crate::run_error(e)),
        };
        if !out.success() {
            return Ok(self.failed_exit(&out, &out_dir, job));
        }
        match validate_output(&out_dir, job, &self.fonts_hash) {
            Ok(png) => Ok(RasterOutcome::Png(png)),
            Err(message) => Ok(RasterOutcome::Failed {
                reason: "raster_output_invalid",
                message,
            }),
        }
    }

    fn failed_exit(
        &self,
        out: &crate::sandbox::ProcOutput,
        out_dir: &Path,
        job: &PageJob<'_>,
    ) -> RasterOutcome {
        // A reported failure (exit 3) is structured; anything else — a signal,
        // an allocation abort under RLIMIT_AS, a panic — is not trusted to say
        // anything beyond its status and a short stderr tail.
        if out.code == Some(3) {
            if let Ok(r) = read_result(out_dir) {
                if r.status == "error" {
                    return RasterOutcome::Failed {
                        reason: "raster_failed",
                        message: format!(
                            "{}: {}",
                            clean(r.kind.as_deref().unwrap_or("unknown"), 40),
                            clean(r.message.as_deref().unwrap_or(""), 500)
                        ),
                    };
                }
            }
        }
        let tail = clean(&String::from_utf8_lossy(&out.stderr), 300);
        let (reason, how) = match out.code {
            None => ("raster_worker_killed", "terminated by a signal".to_string()),
            Some(c) => ("raster_worker_exit", format!("exited with status {c}")),
        };
        RasterOutcome::Failed {
            reason,
            message: format!(
                "worker {how} (address-space limit {} bytes; an allocation past it aborts the worker): {tail}",
                job.address_space
            ),
        }
    }
}

/// Printable, bounded rendering of untrusted text.
fn clean(s: &str, max_chars: usize) -> String {
    let tail: Vec<char> = s.chars().rev().take(max_chars).collect();
    tail.into_iter()
        .rev()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

fn read_result(out_dir: &Path) -> std::result::Result<WorkerResult, String> {
    let md =
        fs::symlink_metadata(out_dir.join(RESULT_NAME)).map_err(|e| format!("result.json: {e}"))?;
    let bytes = read_out_file_bounded(
        out_dir,
        &format!("/out/{RESULT_NAME}"),
        Some(md.len()),
        MAX_RESULT_BYTES,
    )?;
    let r: WorkerResult =
        serde_json::from_slice(&bytes).map_err(|e| format!("result.json: {e}"))?;
    if r.protocol != PROTOCOL {
        return Err(format!(
            "result.json protocol {:?} is not {PROTOCOL}",
            clean(&r.protocol, 40)
        ));
    }
    Ok(r)
}

/// Check the finished `/out` against everything the parent knows
/// independently of the worker's own claims.
fn validate_output(
    out_dir: &Path,
    job: &PageJob<'_>,
    fonts_hash: &str,
) -> std::result::Result<RasterPng, String> {
    let mut seen = BTreeSet::new();
    for entry in fs::read_dir(out_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or("non-UTF-8 output name")?
            .to_string();
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err(format!("output entry {name:?} is not a regular file"));
        }
        seen.insert(name);
    }
    if seen != BTreeSet::from([PNG_NAME.to_string(), RESULT_NAME.to_string()]) {
        return Err(format!(
            "output inventory is {seen:?}, expected exactly {PNG_NAME} and {RESULT_NAME}"
        ));
    }
    let r = read_result(out_dir)?;
    if r.status != "ok" {
        return Err("worker exited 0 without an ok result".into());
    }
    let info = r.png.ok_or("ok result lists no PNG")?;
    if info.file != PNG_NAME {
        return Err("result names an unexpected PNG file".into());
    }
    if info.bytes > job.max_png_bytes {
        return Err(format!(
            "PNG of {} bytes exceeds the {} byte cap",
            info.bytes, job.max_png_bytes
        ));
    }
    if r.svg_sha256.as_deref() != Some(job.svg_sha256) {
        return Err("result is for a different SVG".into());
    }
    if r.fonts_hash.as_deref() != Some(fonts_hash) {
        return Err("result is for a different font snapshot".into());
    }
    let png = read_out_file_bounded(
        out_dir,
        &format!("/out/{PNG_NAME}"),
        Some(info.bytes),
        job.max_png_bytes,
    )?;
    if png.len() < 45 || !png.ends_with(&IEND) {
        return Err("PNG is truncated or lacks an IEND chunk".into());
    }
    let (w, h) = png_dimensions(&png).ok_or("not a PNG with a valid IHDR")?;
    if (w, h) != (info.width, info.height) {
        return Err("PNG dimensions differ from the result".into());
    }
    if u64::from(w) * u64::from(h) > job.max_pixels {
        return Err(format!(
            "PNG {w}x{h} exceeds the {} pixel cap",
            job.max_pixels
        ));
    }
    let sha256 = sha256_hex(&png);
    if sha256 != info.sha256 {
        return Err("PNG SHA-256 differs from the result".into());
    }
    let families = r
        .font_families
        .into_iter()
        .filter(|f| !f.is_empty() && f.len() <= MAX_FAMILY_BYTES)
        .take(MAX_FAMILIES)
        .collect();
    Ok(RasterPng {
        png,
        sha256,
        families,
    })
}
