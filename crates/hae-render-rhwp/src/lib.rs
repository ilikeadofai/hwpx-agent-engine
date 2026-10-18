//! hae-render-rhwp: rhwp preview adapter (spec §9·§10, tasks 3-2…3-4).
//!
//! rhwp runs as a separate process inside a bubblewrap sandbox ([`sandbox`]).
//! The adapter hands it a revision-fixed HWPX snapshot and a copy of the
//! binary through a read-only input directory, reads only the files listed in
//! rhwp's JSON manifest from the temporary `/out`, scans every SVG
//! ([`svg_safety`]) and never falls back to another renderer or to an
//! unsandboxed run. Product PNG rasterisation is never done in this process
//! either: a separate `hae-raster` worker ([`worker`]) runs in the same
//! sandbox on a read-only copy of the page SVG and an exact snapshot of the
//! already hashed fonts. Failure is explicit (`RENDERER_UNAVAILABLE` with a
//! machine-readable reason) and never touches document state.
//!
//! Every tool path is explicit or discovered in a documented order
//! ([`discovery`]): `HAE_RHWP`, `HAE_BWRAP`, `HAE_FONT_DIR` and `HAE_RASTER`
//! always win unchanged, and an unset variable makes the adapter look beside
//! the running executable, in a development tree and on `PATH`. Discovery
//! chooses paths only; it never relaxes the sandbox contract.
//!
//! Windows has no bubblewrap, so previews are refused there by default. An
//! explicit `HAE_ALLOW_UNSANDBOXED_PREVIEW` opt-in runs the renderer without
//! equivalent isolation and marks every result `SANDBOX_UNVERIFIED`
//! ([`unsandboxed`]).
//!
//! Pinned `export-render-tree` has no manifest protocol. The trusted adapter
//! inventories its finished output directory, accepts exactly the bounded
//! `render_tree_NNN.json` set determined by dump-pages, rejects extra entries,
//! and records exact file lengths before any tree read. This exception applies
//! only to that command; SVG paths must always come from rhwp's manifest.

pub mod discovery;
pub mod fonts;
pub mod pages;
pub mod png;
pub mod sandbox;
pub mod svg_safety;
pub mod unsandboxed;
pub mod worker;

use discovery::{DiscoveryFailure, ProbeOutcome, ResolvedTools, SearchPaths};
use fonts::{referenced_families, FontFile, FontSet};
use hae_diag::{Code, Diagnostic, EngineError, Limits, Result, Stage};
use hae_model::Document;
use hae_package::sha256_hex;
pub use pages::{join_pages, PageInfo, PageStatus};
use png::PngOptions;
pub use sandbox::CancelToken;
use sandbox::{ProcOutput, RunError, RunLimits, Sandbox, TempDir};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use unsandboxed::Policy;
use worker::{PageJob, RasterOutcome, RasterStage};

/// rhwp commit this engine pins (spec §9).
pub const PINNED_COMMIT: &str = "1a76570e833917d15817415a53c09ad61ab3203f";
/// Default bubblewrap location, used when `HAE_BWRAP` is unset and discovery
/// finds nothing earlier (the first, documented candidate).
pub const DEFAULT_BWRAP: &str = "/usr/bin/bwrap";
/// Default font directory when `HAE_FONT_DIR` is unset and discovery finds no
/// font directory at all (the first, documented candidate).
pub const DEFAULT_FONT_DIR: &str = "/usr/share/fonts";

const INPUT_NAME: &str = "doc.hwpx";
const MAX_RENDER_TREE_PAGES: usize = 2000;
const MAX_OUT_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_NOTES: usize = 200;

/// Adapter configuration. Every path is explicit or discovered.
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    pub rhwp_path: PathBuf,
    pub bwrap_path: PathBuf,
    pub font_dir: PathBuf,
    /// `hae-raster` worker executable used for every PNG. Defaults to
    /// `hae-raster` beside the running executable
    /// ([`worker::default_worker_path`]); `HAE_RASTER` overrides it.
    pub raster_path: PathBuf,
    /// Parent of the per-run temporary directories.
    pub work_dir: PathBuf,
    /// rhwp commit recorded in metadata (as configured, not derived).
    pub renderer_commit: Option<String>,
    /// Timeout (`renderer_timeout_secs`) and page limit (`max_preview_pages`).
    pub limits: Limits,
    pub stdout_cap: usize,
    pub stderr_cap: usize,
    pub out_cap_bytes: u64,
    /// Provenance of every path above: which rule produced it and which
    /// candidates were examined first. Recorded by `doctor` and by failure
    /// diagnostics so a missing tool says what to do.
    pub tools: ResolvedTools,
    /// Windows opt-in for running the renderer without verified isolation
    /// ([`unsandboxed`]). Always `Off` unless the environment asks for it.
    pub unsandboxed_preview: Policy,
}

impl AdapterConfig {
    pub fn new(rhwp_path: impl Into<PathBuf>, font_dir: impl Into<PathBuf>) -> Self {
        let rhwp_path = rhwp_path.into();
        let font_dir = font_dir.into();
        let bwrap_path = PathBuf::from(DEFAULT_BWRAP);
        let raster_path = worker::default_worker_path();
        AdapterConfig {
            tools: ResolvedTools::all_explicit(&rhwp_path, &bwrap_path, &font_dir, &raster_path),
            unsandboxed_preview: Policy::Off,
            rhwp_path,
            bwrap_path,
            font_dir,
            raster_path,
            work_dir: std::env::temp_dir(),
            renderer_commit: Some(PINNED_COMMIT.to_string()),
            limits: Limits::default(),
            stdout_cap: 64 * 1024 * 1024,
            stderr_cap: 8 * 1024 * 1024,
            out_cap_bytes: 512 * 1024 * 1024,
        }
    }

    /// Read `HAE_RHWP`, `HAE_BWRAP`, `HAE_FONT_DIR` and `HAE_RASTER`, and fall
    /// back to the documented search order for whatever is unset
    /// ([`discovery`]). `None` when no renderer could be found or configured,
    /// i.e. the preview adapter is not configured.
    pub fn from_env() -> Option<Self> {
        Self::from_env_detailed().ok()
    }

    /// [`Self::from_env`] with the reason a renderer was not found: which
    /// candidates were examined and what fixes it.
    pub fn from_env_detailed() -> std::result::Result<Self, DiscoveryFailure> {
        let env = preview_environment();
        let Some(rhwp) = env.tools.rhwp.path.clone() else {
            return Err(env
                .tools
                .rhwp_failure()
                .unwrap_or_else(|| DiscoveryFailure {
                    tool: "rhwp",
                    message: format!(
                        "rhwp renderer executable not found: set HAE_RHWP to its path, or place \
                         it beside this application (or on PATH) (see {})",
                        discovery::DISCOVERY_ENV
                    ),
                    searched: Vec::new(),
                }));
        };
        let mut c = AdapterConfig::new(
            rhwp,
            env.tools
                .font_dir
                .path
                .clone()
                .map_or_else(|| PathBuf::from(DEFAULT_FONT_DIR), PathBuf::from),
        );
        c.bwrap_path = env
            .tools
            .bwrap
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_BWRAP));
        c.raster_path = env
            .tools
            .raster
            .path
            .clone()
            .unwrap_or_else(worker::default_worker_path);
        c.tools = env.tools;
        c.unsandboxed_preview = env.unsandboxed_policy;
        Ok(c)
    }

    /// Where the current `rhwp_path` came from: the recorded rule, or
    /// `explicit` when a caller replaced the path after construction.
    pub fn rhwp_source(&self) -> &'static str {
        source_of(&self.tools.rhwp, &self.rhwp_path)
    }

    /// Where the current `bwrap_path` came from.
    pub fn bwrap_source(&self) -> &'static str {
        source_of(&self.tools.bwrap, &self.bwrap_path)
    }

    /// Where the current `font_dir` came from.
    pub fn font_source(&self) -> &'static str {
        // Nothing usable was found: the documented default is only a
        // placeholder that the next scan will explain, not a chosen source.
        if self.tools.font_dir.path.is_none() && self.font_dir == Path::new(DEFAULT_FONT_DIR) {
            return "unresolvedDefault";
        }
        source_of(&self.tools.font_dir, &self.font_dir)
    }

    /// Where the current `raster_path` came from.
    pub fn raster_source(&self) -> &'static str {
        source_of(&self.tools.raster, &self.raster_path)
    }

    /// Why previews will still be refused on this platform even though a
    /// renderer was configured: non-Unix has no verified isolation, and the
    /// Windows opt-in is off. `None` when previews can run.
    pub fn platform_isolation_note(&self) -> Option<String> {
        if cfg!(unix) || (cfg!(windows) && self.unsandboxed_preview.is_on()) {
            return None;
        }
        Some(unsandboxed::refusal_message())
    }

    /// `RENDERER_UNAVAILABLE(renderer_not_configured)` for an adapter with no
    /// renderer path, carrying the discovery result and the fix.
    fn unconfigured_error(&self) -> EngineError {
        match self.tools.rhwp_failure() {
            Some(f) => f.to_error(),
            None => unavailable(
                "renderer_not_configured",
                format!(
                    "no rhwp renderer is configured: set HAE_RHWP to the rhwp executable path, or \
                     enable discovery ({}=1) and place it beside this application or on PATH",
                    discovery::DISCOVERY_ENV
                ),
            ),
        }
    }

    fn run_limits(&self) -> RunLimits {
        let mut l = RunLimits::with_timeout_secs(self.limits.renderer_timeout_secs);
        l.stdout_cap = self.stdout_cap;
        l.stderr_cap = self.stderr_cap;
        l.out_cap_bytes = self.out_cap_bytes;
        l
    }
}

/// Provenance of a configured path: the recorded discovery rule, or
/// `explicit` when the path no longer matches what discovery produced.
fn source_of(probe: &ProbeOutcome, path: &Path) -> &'static str {
    if probe.path.as_deref() == Some(path) {
        probe.source_str()
    } else {
        "explicit"
    }
}

/// Everything the preview adapter would use, without building an adapter and
/// without running anything: resolved tool paths with provenance, the Windows
/// opt-in policy and what that policy can and cannot enforce.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEnvironment {
    pub tools: ResolvedTools,
    pub unsandboxed_policy: Policy,
    pub enforcement: Vec<unsandboxed::Enforcement>,
}

/// Resolve the preview environment from the real process environment.
pub fn preview_environment() -> PreviewEnvironment {
    let paths = SearchPaths::from_process();
    let overrides = discovery::Overrides::from_process();
    PreviewEnvironment {
        tools: ResolvedTools::resolve(&overrides, &paths),
        unsandboxed_policy: Policy::from_process(),
        enforcement: unsandboxed::enforcement(),
    }
}

pub(crate) fn unavailable(reason: &str, msg: impl Into<String>) -> EngineError {
    EngineError::new(Code::RendererUnavailable, Stage::Render, msg).with_reason(reason)
}

fn run_error(e: RunError) -> EngineError {
    unavailable(e.reason(), e.to_string())
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path)
    }
}

fn set_renderer_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }
    #[cfg(windows)]
    {
        // Windows has no execute bit; the staged copy is the renderer.
        let _ = path;
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "renderer staging requires Unix executable permissions",
        ))
    }
}

/// What one preview run is asked to render.
#[derive(Debug, Clone)]
pub struct RenderRequest<'a> {
    pub document_id: &'a str,
    /// Content revision of the snapshot (equals the document revision).
    pub content_revision: u64,
    /// Revision-fixed HWPX bytes (`Document::to_hwpx`).
    pub hwpx: &'a [u8],
    /// 1-based pages to return; `None` returns every rendered page.
    pub pages: Option<&'a [u32]>,
    /// Also rasterise each returned page to PNG.
    pub want_png: bool,
    pub png: PngOptions,
}

/// Overall outcome (spec §10). `rendered` does not mean "matches Hancom".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderStatus {
    Rendered,
    Partial,
    Failed,
}

/// Renderer-level note (layout warnings, dropped pages, font gaps). The
/// engine's diagnostic code list has no entries for these, so they carry
/// their own string code.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderNote {
    pub code: String,
    pub severity: &'static str,
    /// 1-based page, when the note concerns one page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub message: String,
}

fn note(
    code: &str,
    severity: &'static str,
    page: Option<u32>,
    message: impl Into<String>,
) -> RenderNote {
    RenderNote {
        code: code.to_string(),
        severity,
        page,
        message: message.into(),
    }
}

#[derive(Debug, Clone)]
pub struct PreviewPage {
    /// 1-based physical page number.
    pub page: u32,
    pub svg: Vec<u8>,
    pub svg_sha256: String,
    pub png: Option<Vec<u8>>,
    pub png_sha256: Option<String>,
}

/// Preview metadata recorded with every result (spec §10).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMetadata {
    pub document_id: String,
    pub content_revision: u64,
    pub layout_revision: String,
    pub input_sha256: String,
    pub renderer: Value,
    pub sandbox: Value,
    pub fonts: Value,
    pub page_conditions: Value,
    pub coverage: &'static str,
    pub native_comparison: &'static str,
}

#[derive(Debug, Clone)]
pub struct PreviewOutput {
    pub status: RenderStatus,
    /// Pages in the document as reported by rhwp (before the page limit).
    pub total_pages: usize,
    pub pages: Vec<PreviewPage>,
    pub notes: Vec<RenderNote>,
    /// Engine diagnostics (`PREVIEW_PAGE_LIMIT`).
    pub diagnostics: Vec<Diagnostic>,
    pub metadata: PreviewMetadata,
    pub render_millis: u128,
}

/// Explicit allocation accounting. Parser multipliers reserve workspace before
/// parsing; they are deliberately not advertised as a process RSS guarantee.
struct MemoryBudget {
    remaining: u64,
}
impl MemoryBudget {
    fn reserve(&mut self, bytes: u64) -> Result<()> {
        self.remaining = self.remaining.checked_sub(bytes).ok_or_else(|| {
            unavailable(
                "memory_limit",
                format!("preview needs {bytes} bytes; {} remain", self.remaining),
            )
        })?;
        Ok(())
    }
    fn release(&mut self, bytes: u64) {
        self.remaining = self.remaining.saturating_add(bytes);
    }
}

#[derive(Default)]
struct State {
    #[cfg(unix)]
    sandbox_verified: bool,
    fonts: Option<Arc<FontSet>>,
}

/// The rhwp adapter. Cheap to share behind an `Arc`.
pub struct RhwpAdapter {
    cfg: AdapterConfig,
    state: Mutex<State>,
}

/// Process runner for every renderer command: the verified bubblewrap sandbox
/// on Unix, or the explicitly opted-in unsandboxed runner on Windows.
enum Runner {
    #[cfg_attr(not(unix), allow(dead_code))]
    Sandbox(Sandbox),
    #[cfg(windows)]
    Unsandboxed(unsandboxed::Runner),
}

impl Runner {
    /// Run `argv` with `limits`. `memory_bytes` is the `RLIMIT_AS` value of the
    /// supervised process on Unix; the unsandboxed Windows path cannot apply an
    /// equivalent and records `applied: false` in the result metadata.
    fn run_with_memory(
        &self,
        in_dir: &Path,
        out_dir: &Path,
        argv: &[&str],
        limits: &RunLimits,
        cancel: Option<&CancelToken>,
        memory_bytes: Option<u64>,
    ) -> std::result::Result<ProcOutput, RunError> {
        match self {
            Runner::Sandbox(sb) => {
                sb.run_with_memory(in_dir, out_dir, argv, limits, cancel, memory_bytes)
            }
            #[cfg(windows)]
            Runner::Unsandboxed(r) => {
                r.run_with_memory(in_dir, out_dir, argv, limits, cancel, memory_bytes)
            }
        }
    }

    /// True only for the verified Linux bubblewrap sandbox.
    fn is_verified_sandbox(&self) -> bool {
        match self {
            Runner::Sandbox(_) => true,
            #[cfg(windows)]
            Runner::Unsandboxed(_) => false,
        }
    }
}

struct Staged {
    _td: TempDir,
    in_dir: PathBuf,
    out_dir: PathBuf,
    rhwp_sha256: String,
    input_sha256: String,
}

impl RhwpAdapter {
    pub fn new(cfg: AdapterConfig) -> Self {
        RhwpAdapter {
            cfg,
            state: Mutex::new(State::default()),
        }
    }

    pub fn config(&self) -> &AdapterConfig {
        &self.cfg
    }

    /// Fonts of the configured directory (rescanned when the listing changed).
    pub fn fonts(&self) -> Result<Arc<FontSet>> {
        self.fonts_with_budget(&mut MemoryBudget {
            remaining: self.cfg.limits.max_session_memory,
        })
    }

    fn fonts_with_budget(&self, budget: &mut MemoryBudget) -> Result<Arc<FontSet>> {
        let mut st = self.state.lock().unwrap();
        if let Some(f) = &st.fonts {
            if f.is_current() {
                budget.reserve(f.memory_reservation())?;
                return Ok(f.clone());
            }
        }
        st.fonts = None;
        // Reserve before hashing/loading, then retain the actual bounded charge.
        let allowance = budget.remaining;
        budget.reserve(allowance)?;
        let f = Arc::new(
            FontSet::scan_with_budget(
                &self.cfg.font_dir,
                allowance.saturating_sub(1024 * 1024) / 4,
            )
            .map_err(|e| self.font_scan_error(&e))?,
        );
        let charge = f.memory_reservation();
        budget.release(allowance);
        budget.reserve(charge)?;
        st.fonts = Some(f.clone());
        Ok(f)
    }

    /// The error for a font directory that could not be loaded. A directory
    /// that fails the plain font limits (or cannot be read) is
    /// `font_dir_unusable` and names the directory, where it came from and
    /// how to fix it; only a directory that passes them but does not fit the
    /// remaining session allowance is `memory_limit`.
    fn font_scan_error(&self, e: &std::io::Error) -> EngineError {
        let limits_error = fonts::inspect_font_dir(&self.cfg.font_dir).error;
        let reason = if limits_error.is_some() {
            "font_dir_unusable"
        } else {
            "memory_limit"
        };
        unavailable(
            reason,
            format!(
                "font directory {} [{}] cannot be used: {e}; limits: at most {} files, {} bytes \
                 in total, {} bytes per file. Set HAE_FONT_DIR to a directory of only the fonts \
                 you need ({})",
                self.cfg.font_dir.display(),
                self.cfg.font_source(),
                fonts::MAX_FONT_FILES,
                fonts::MAX_FONT_BYTES,
                fonts::MAX_FONT_FILE_BYTES,
                self.cfg.tools.font_note()
            ),
        )
    }

    /// Verify isolation once and return the process runner.
    ///
    /// Unix: a missing bwrap, a failed isolation self-test or an unusable font
    /// mount means `sandbox_unverified` and the renderer is never started.
    /// Windows: refused unless the explicit unsandboxed opt-in is set.
    #[cfg(unix)]
    fn runner(&self) -> Result<Runner> {
        let sb = Sandbox::new(&self.cfg.bwrap_path, &self.cfg.font_dir);
        let mut st = self.state.lock().unwrap();
        if !st.sandbox_verified {
            if !self.cfg.font_dir.is_dir() {
                return Err(unavailable(
                    "sandbox_unverified",
                    format!(
                        "font directory {} is not a directory [{}]: set HAE_FONT_DIR to a readable \
                         directory of fonts ({})",
                        self.cfg.font_dir.display(),
                        self.cfg.font_source(),
                        self.cfg.tools.font_note()
                    ),
                ));
            }
            sb.self_test(&self.cfg.work_dir).map_err(|m| {
                unavailable(
                    "sandbox_unverified",
                    format!(
                        "sandbox isolation could not be verified: {m}; {}",
                        self.sandbox_hint()
                    ),
                )
            })?;
            st.sandbox_verified = true;
        }
        Ok(Runner::Sandbox(sb))
    }

    /// One line naming the bubblewrap that was used, what to install instead,
    /// and the fact that the Linux path never accepts an unsandboxed opt-in.
    #[cfg(unix)]
    fn sandbox_hint(&self) -> String {
        let mut s = format!(
            "bwrap {} [{}]",
            self.cfg.bwrap_path.display(),
            self.cfg.bwrap_source()
        );
        if !self.cfg.tools.bwrap.searched.is_empty() {
            s.push_str(&format!(
                "; searched: {}",
                self.cfg.tools.bwrap.searched_str(8)
            ));
        }
        s.push_str("; set HAE_BWRAP to a working bubblewrap or install it at /usr/bin/bwrap");
        if self.cfg.unsandboxed_preview.is_on() {
            s.push_str(
                "; HAE_ALLOW_UNSANDBOXED_PREVIEW is only honored on Windows, so Linux previews still \
                 require verified bubblewrap isolation",
            );
        }
        s
    }

    #[cfg(windows)]
    fn runner(&self) -> Result<Runner> {
        if !self.cfg.unsandboxed_preview.is_on() {
            return Err(unavailable(
                "sandbox_unverified",
                unsandboxed::refusal_message(),
            ));
        }
        Ok(Runner::Unsandboxed(unsandboxed::Runner::new()))
    }

    #[cfg(not(any(unix, windows)))]
    fn runner(&self) -> Result<Runner> {
        Err(unavailable(
            "sandbox_unverified",
            sandbox::unsupported_platform_message(),
        ))
    }

    fn stage(&self, hwpx: &[u8]) -> Result<Staged> {
        let mut rhwp = fs::File::open(&self.cfg.rhwp_path).map_err(|e| {
            unavailable(
                "renderer_missing",
                format!(
                    "rhwp binary {} [{}]: {e}; set HAE_RHWP to an existing, executable rhwp or \
                     place it beside this application",
                    self.cfg.rhwp_path.display(),
                    self.cfg.rhwp_source()
                ),
            )
        })?;
        let io = |what: &str, e: std::io::Error| {
            EngineError::new(Code::IoError, Stage::Render, format!("{what}: {e}"))
        };
        let td =
            TempDir::new_in(&self.cfg.work_dir, "hae-render-").map_err(|e| io("work dir", e))?;
        let in_dir = td.path().join("in");
        let out_dir = td.path().join("out");
        for d in [&in_dir, &out_dir] {
            create_private_dir(d).map_err(|e| io("mkdir", e))?;
        }
        let bin = in_dir.join("rhwp");
        let mut staged_bin = fs::File::create(&bin).map_err(|e| io("create renderer", e))?;
        let mut hash = Sha256::new();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = rhwp.read(&mut chunk).map_err(|e| io("read renderer", e))?;
            if n == 0 {
                break;
            }
            staged_bin
                .write_all(&chunk[..n])
                .map_err(|e| io("copy renderer", e))?;
            hash.update(&chunk[..n]);
        }
        let rhwp_sha256 = format!("{:x}", hash.finalize());
        set_renderer_executable(&bin).map_err(|e| io("chmod", e))?;
        fs::write(in_dir.join(INPUT_NAME), hwpx).map_err(|e| io("write snapshot", e))?;
        Ok(Staged {
            rhwp_sha256,
            input_sha256: sha256_hex(hwpx),
            in_dir,
            out_dir,
            _td: td,
        })
    }

    fn preview_run(
        &self,
        runner: &Runner,
        st: &Staged,
        argv: &[&str],
        cancel: Option<&CancelToken>,
        budget: &MemoryBudget,
        deadline: Instant,
    ) -> Result<ProcOutput> {
        let mut limits = self.cfg.run_limits();
        limits.timeout = deadline.saturating_duration_since(Instant::now());
        if limits.timeout.is_zero() {
            return Err(run_error(RunError::Timeout(limits.timeout)));
        }
        // Pipe capacities are reserved by subdivision before readers allocate.
        limits.stdout_cap = limits.stdout_cap.min((budget.remaining / 32) as usize);
        limits.stderr_cap = limits.stderr_cap.min((budget.remaining / 128) as usize);
        let out = runner
            .run_with_memory(
                &st.in_dir,
                &st.out_dir,
                argv,
                &limits,
                cancel,
                Some(budget.remaining),
            )
            .map_err(run_error)?;
        if !out.success() {
            let tail: String = String::from_utf8_lossy(&out.stderr)
                .chars()
                .rev()
                .take(1000)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            return Err(unavailable(
                "render_failed",
                format!("renderer exited with {:?} for {:?}", out.code, argv.get(1)),
            )
            .with_detail(json!({"stderrTail":tail})));
        }
        Ok(out)
    }

    /// Backwards-compatible entry point using the configured session limit.
    pub fn preview(
        &self,
        req: &RenderRequest<'_>,
        cancel: Option<&CancelToken>,
    ) -> Result<PreviewOutput> {
        self.preview_with_budget(req, cancel, self.cfg.limits.max_session_memory)
    }

    /// Render against the session's remaining allowance (after document/history
    /// charges). Reservations include retained pages, fonts and parse workspace.
    pub fn preview_with_budget(
        &self,
        req: &RenderRequest<'_>,
        cancel: Option<&CancelToken>,
        memory_budget: u64,
    ) -> Result<PreviewOutput> {
        let started = Instant::now();
        if self.cfg.rhwp_path.as_os_str().is_empty() {
            return Err(self.cfg.unconfigured_error());
        }
        let runner = self.runner()?;
        let deadline = Instant::now() + Duration::from_secs(self.cfg.limits.renderer_timeout_secs);
        let mut budget = MemoryBudget {
            remaining: memory_budget.min(self.cfg.limits.max_session_memory),
        };
        // Fixed metadata, bounded notes, staging buffers, and borrowed snapshot.
        let request_charge = req
            .pages
            .map_or(0, |p| p.len() as u64)
            .checked_mul(64)
            .and_then(|n| {
                n.checked_add(
                    req.hwpx.len() as u64 + req.document_id.len() as u64 * 8 + 1024 * 1024,
                )
            })
            .ok_or_else(|| unavailable("memory_limit", "request metadata size overflow"))?;
        budget.reserve(request_charge)?;
        let fonts = self.fonts_with_budget(&mut budget)?;
        let st = self.stage(req.hwpx)?;
        // PNGs are made only by the supervised worker, which needs the same
        // verified sandbox. An unsandboxed run keeps the SVG and reports the
        // missing PNG instead of rasterising on the host.
        let sandboxed = runner.is_verified_sandbox();
        let mut raster = if req.want_png && sandboxed {
            Some(RasterStage::prepare(
                &self.cfg.raster_path,
                &self.cfg.bwrap_path,
                &self.cfg.work_dir,
                &fonts,
            )?)
        } else {
            None
        };
        let mut raster_families: Option<BTreeSet<String>> = None;
        let mut raster_address_space = 0u64;
        let version_out = self.preview_run(
            &runner,
            &st,
            &["/in/rhwp", "--version"],
            cancel,
            &budget,
            deadline,
        )?;
        let version = String::from_utf8_lossy(&version_out.stdout)
            .trim()
            .to_string();
        if !version.starts_with("rhwp") || version.len() > 200 || version.contains('\n') {
            return Err(unavailable(
                "invalid_output",
                "unrecognised renderer version output",
            ));
        }
        drop(version_out);
        let dump_out = self.preview_run(
            &runner,
            &st,
            &["/in/rhwp", "dump-pages", "/in/doc.hwpx", "--json"],
            cancel,
            &budget,
            deadline,
        )?;
        let (dump, dump_charge) = parse_output_json(&dump_out, &mut budget)?;
        let total = dump
            .get("pages")
            .and_then(Value::as_array)
            .map(Vec::len)
            .ok_or_else(|| unavailable("invalid_output", "dump-pages output has no pages array"))?;
        drop(dump);
        drop(dump_out);
        budget.release(dump_charge);
        let max_pages = self.cfg.limits.max_preview_pages as usize;
        let limit_hit = total > max_pages;
        let rendered_count = total.min(max_pages);
        let mut notes = Vec::new();
        let mut diagnostics = Vec::new();
        if !sandboxed {
            if notes.len() < MAX_NOTES {
                notes.push(note(
                    "SANDBOX_UNVERIFIED",
                    "warning",
                    None,
                    format!(
                        "the renderer ran without verified isolation ({}): this is not equivalent \
                         to Linux bubblewrap isolation and none of its filesystem, network or memory \
                         guarantees apply",
                        unsandboxed::ALLOW_ENV
                    ),
                ));
            }
            if req.want_png && notes.len() < MAX_NOTES {
                notes.push(note(
                    "PNG_UNAVAILABLE",
                    "warning",
                    None,
                    "PNG rasterisation needs the isolated hae-raster worker, which is Linux-only; \
                     SVG pages are returned instead",
                ));
            }
        }
        if limit_hit {
            diagnostics.push(
                Diagnostic::error(
                    Code::PreviewPageLimit,
                    Stage::Render,
                    format!(
                        "document has {total} pages; export is restricted to the first {max_pages}"
                    ),
                )
                .with_detail(json!({"totalPages":total,"maxPages":max_pages})),
            );
        }
        let selected_count = req
            .pages
            .map_or(rendered_count, |pages| pages.len().min(rendered_count));
        budget.reserve(
            (selected_count as u64)
                .checked_mul(16384)
                .ok_or_else(|| unavailable("memory_limit", "page metadata size overflow"))?,
        )?;
        let wanted: Vec<usize> = match req.pages {
            None => (0..rendered_count).collect(),
            Some(list) => {
                let mut seen = BTreeSet::new();
                for &p in list {
                    if p >= 1 && (p as usize) <= rendered_count {
                        seen.insert(p as usize - 1);
                    } else if notes.len() < MAX_NOTES {
                        notes.push(note(
                            "PAGE_NOT_RENDERED",
                            "warning",
                            Some(p),
                            format!("page {p} is outside the rendered range 1..={rendered_count}"),
                        ));
                    }
                }
                seen.into_iter().collect()
            }
        };
        let missing_requested = req
            .pages
            .is_some_and(|l| l.iter().any(|&p| p == 0 || p as usize > rendered_count));
        let mut pages_out = Vec::new();
        let mut dropped_unsafe = 0usize;
        let mut families = BTreeSet::new();
        let mut seen_paths = BTreeSet::new();
        for idx in wanted {
            if cancel.is_some_and(CancelToken::is_cancelled) {
                return Err(run_error(RunError::Cancelled));
            }
            let page_arg = idx.to_string();
            // Pinned rhwp supports one zero-based --page per invocation, not a
            // range. Its pageCount remains the document total for a subset.
            let out = self.preview_run(
                &runner,
                &st,
                &[
                    "/in/rhwp",
                    "export-svg",
                    "/in/doc.hwpx",
                    "-o",
                    "/out",
                    "--json",
                    "--page",
                    &page_arg,
                ],
                cancel,
                &budget,
                deadline,
            )?;
            let (manifest, charge) = parse_output_json(&out, &mut budget)?;
            if manifest.get("pageCount").and_then(Value::as_u64) != Some(total as u64) {
                return Err(unavailable(
                    "invalid_output",
                    "manifest total differs from bounded dump-pages",
                ));
            }
            let listed = parse_manifest(&manifest).map_err(|m| unavailable("invalid_output", m))?;
            if listed.len() != 1 || listed[0].page != idx {
                return Err(unavailable(
                    "invalid_output",
                    "renderer did not export exactly the selected page",
                ));
            }
            let lp = &listed[0];
            if !seen_paths.insert(lp.path.clone()) {
                return Err(unavailable(
                    "invalid_output",
                    "duplicate manifest output path",
                ));
            }
            if notes.len() < MAX_NOTES {
                notes.extend(
                    layout_notes(&String::from_utf8_lossy(&out.stderr))
                        .into_iter()
                        .take(MAX_NOTES - notes.len()),
                );
            }
            if manifest.get("untrustedContent").and_then(Value::as_bool) == Some(true)
                && notes.len() < MAX_NOTES
            {
                notes.push(note(
                    "RENDERER_UNTRUSTED_CONTENT",
                    "warning",
                    None,
                    "rhwp flagged the document content as untrusted",
                ));
            }
            if manifest
                .get("overflowCellLines")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
                && notes.len() < MAX_NOTES
            {
                notes.push(note(
                    "LAYOUT_OVERFLOW_CELL_LINES",
                    "warning",
                    Some(idx as u32 + 1),
                    "rhwp reported overflowing table cell lines",
                ));
            }
            let path = lp.path.clone();
            let bytes_len = lp.bytes;
            drop(listed);
            drop(manifest);
            drop(out);
            budget.release(charge);
            let workspace = bytes_len
                .checked_mul(16)
                .and_then(|n| n.checked_add(65536))
                .ok_or_else(|| unavailable("memory_limit", "SVG workspace size overflow"))?;
            budget.reserve(workspace)?;
            let bytes = read_out_file_bounded(&st.out_dir, &path, Some(bytes_len), bytes_len)
                .map_err(|m| unavailable("invalid_output", m))?;
            let chk = svg_safety::check_svg(&bytes);
            if !chk.is_safe() {
                dropped_unsafe += 1;
                if notes.len() < MAX_NOTES {
                    notes.push(note(
                        "SVG_UNSAFE",
                        "error",
                        Some(idx as u32 + 1),
                        format!("page withheld, unsafe SVG ({} issues)", chk.issues.len()),
                    ));
                }
                drop(bytes);
                budget.release(workspace);
                continue;
            }
            if let Ok(text) = std::str::from_utf8(&bytes) {
                // Cap retained metadata across all pages.
                for family in referenced_families(text).into_iter().take(50) {
                    if families.len() < 50 && family.len() <= 200 {
                        families.insert(family);
                    }
                }
            }
            let mut page = PreviewPage {
                page: idx as u32 + 1,
                svg_sha256: sha256_hex(&bytes),
                svg: bytes,
                png: None,
                png_sha256: None,
            };
            if let Some(stage) = raster.as_mut() {
                let mut opts = req.png;
                opts.max_pixels = opts.max_pixels.min(self.cfg.limits.max_image_pixels);
                // The parent keeps only the encoded PNG (cap reserved up front)
                // and a bounded result read. The rest of the remaining session
                // allowance is the worker's RLIMIT_AS (virtual address space).
                let png_cap = (budget.remaining / 8)
                    .min(MAX_OUT_FILE_BYTES)
                    .min(self.cfg.out_cap_bytes);
                let held = png_cap + worker::RESULT_WORKSPACE;
                budget.reserve(held)?;
                let address_space = budget.remaining;
                if address_space < worker::MIN_ADDRESS_SPACE {
                    return Err(unavailable(
                        "memory_limit",
                        format!(
                            "{address_space} bytes remain for the raster worker; it needs at least {}",
                            worker::MIN_ADDRESS_SPACE
                        ),
                    ));
                }
                raster_address_space = raster_address_space.max(address_space);
                let job = PageJob {
                    svg: &page.svg,
                    svg_sha256: &page.svg_sha256,
                    scale: opts.scale,
                    max_pixels: opts.max_pixels,
                    max_decoded_bytes: address_space / 4,
                    max_png_bytes: png_cap,
                    address_space,
                };
                match stage.rasterize(&job, cancel, deadline)? {
                    RasterOutcome::Png(p) => {
                        budget.release(held - p.png.len() as u64);
                        raster_families.get_or_insert(p.families);
                        page.png_sha256 = Some(p.sha256);
                        page.png = Some(p.png);
                    }
                    RasterOutcome::Failed { reason, message } => {
                        budget.release(held);
                        if notes.len() < MAX_NOTES {
                            notes.push(note(
                                "PNG_FAILED",
                                "warning",
                                Some(page.page),
                                format!("raster worker {reason}: {message}"),
                            ));
                        }
                    }
                }
            }
            budget.release(workspace - page.svg.len() as u64);
            pages_out.push(page);
        }

        // Family availability is known only from a worker that loaded the
        // snapshot; the host never builds a font database for the preview.
        if let Some(available) = raster_families.as_ref().filter(|_| !families.is_empty()) {
            for f in families.iter().take(50) {
                if !available.contains(&f.to_lowercase()) {
                    notes.push(note(
                        "FONT_UNAVAILABLE",
                        "warning",
                        None,
                        format!("font family '{f}' is not in the configured font directory; raster output substitutes it"),
                    ));
                }
            }
        }
        if notes.len() > MAX_NOTES {
            let extra = notes.len() - MAX_NOTES;
            notes.truncate(MAX_NOTES);
            notes.push(note(
                "NOTES_TRUNCATED",
                "info",
                None,
                format!("{extra} more notes omitted"),
            ));
        }

        let status = if pages_out.is_empty() {
            RenderStatus::Failed
        } else if limit_hit
            || dropped_unsafe > 0
            || missing_requested
            || (req.want_png && pages_out.iter().any(|p| p.png.is_none()))
        {
            // A requested PNG that could not be made (worker failure, cap) is
            // an incomplete result even though the SVG page is retained.
            RenderStatus::Partial
        } else {
            RenderStatus::Rendered
        };

        // Layout revision: identifies this input + environment + page conditions.
        let cond =
            json!({ "maxPages": max_pages, "requestedPages": req.pages, "format": "svg-v1" });
        let layout_revision = format!(
            "lay-{}",
            &sha256_hex(
                format!(
                    "{}|{}|{}|{}",
                    st.input_sha256, st.rhwp_sha256, fonts.hash, cond
                )
                .as_bytes()
            )[..16]
        );
        let bwrap_version = if sandboxed {
            bwrap_version(&self.cfg.bwrap_path, &st.out_dir, deadline, cancel, &budget)
        } else {
            None
        };
        if cancel.is_some_and(CancelToken::is_cancelled) {
            return Err(run_error(RunError::Cancelled));
        }
        // The Linux contract is reported exactly as before. The unsandboxed
        // path says what it enforces and what it does not; it never claims
        // `selfTest: passed`.
        let sandbox_meta = if sandboxed {
            json!({ "tool": "bubblewrap", "version": bwrap_version, "selfTest": "passed",
                "memoryLimit": {"tool":"/usr/bin/prlimit", "resource":"RLIMIT_AS", "scope":"per-process virtual address space; inherited by descendants", "bytesAtMost":memory_budget.min(self.cfg.limits.max_session_memory)},
                "rasterMemoryLimit": raster.as_ref().map(|_| json!({"tool":"/usr/bin/prlimit", "resource":"RLIMIT_AS", "scope":"raster worker process only: virtual address space after parent PNG/result reservations; not RSS, not an aggregate with the parent, tmpfs or kernel memory", "bytesAtMost":raster_address_space})),
            })
        } else {
            unsandboxed::describe(
                memory_budget.min(self.cfg.limits.max_session_memory),
                Duration::from_secs(self.cfg.limits.renderer_timeout_secs),
            )
        };
        let metadata = PreviewMetadata {
            document_id: req.document_id.to_string(),
            content_revision: req.content_revision,
            layout_revision,
            input_sha256: st.input_sha256.clone(),
            renderer: json!({
                "name": "rhwp",
                "version": version,
                "sha256": st.rhwp_sha256,
                "commit": self.cfg.renderer_commit,
                "rasterizer": raster.as_ref().map(|r| json!({
                    "name": worker::WORKER_NAME,
                    "protocol": worker::PROTOCOL,
                    "sha256": r.worker_sha256,
                    "bytes": r.worker_bytes,
                    "fontSnapshot": {"hash": r.fonts_hash, "files": r.font_files},
                })),
            }),
            sandbox: sandbox_meta,
            fonts: json!({
                "directory": self.cfg.font_dir,
                "source": self.cfg.font_source(),
                "selection": self.cfg.tools.font_dir.note,
                "searched": self.cfg.tools.font_dir.searched,
                "hash": fonts.hash,
                "files": fonts.files.iter().map(|f: &FontFile| json!({"path": f.path, "bytes": f.bytes, "sha256": f.sha256})).collect::<Vec<_>>(),
            }),
            page_conditions: json!({
                "maxPages": max_pages,
                "requestedPages": req.pages,
                "returnedPages": pages_out.iter().map(|p| p.page).collect::<Vec<_>>(),
                "totalPages": total,
            }),
            coverage: "unknown",
            native_comparison: "not_checked",
        };
        Ok(PreviewOutput {
            status,
            total_pages: total,
            pages: pages_out,
            notes,
            diagnostics,
            metadata,
            render_millis: started.elapsed().as_millis(),
        })
    }

    /// Page numbers for the document's nodes (spec §7), computed from the
    /// document's current snapshot. Pages are `renderer-estimated`.
    pub fn page_info(&self, doc: &Document, cancel: Option<&CancelToken>) -> Result<PageInfo> {
        let runner = self.runner()?;
        let deadline = Instant::now() + Duration::from_secs(self.cfg.limits.renderer_timeout_secs);
        let mut budget = MemoryBudget {
            remaining: self.cfg.limits.max_session_memory,
        };
        budget.reserve(1024 * 1024 + doc.state.total_bytes().saturating_mul(2))?;
        let hwpx = doc.to_hwpx()?;
        let st = self.stage(&hwpx)?;
        let dump_out = self.preview_run(
            &runner,
            &st,
            &["/in/rhwp", "dump-pages", "/in/doc.hwpx", "--json"],
            cancel,
            &budget,
            deadline,
        )?;
        let (dump, _dump_charge) = parse_output_json(&dump_out, &mut budget)?;
        drop(dump_out);
        let n = dump
            .get("pages")
            .and_then(Value::as_array)
            .map(Vec::len)
            .ok_or_else(|| unavailable("invalid_output", "dump-pages output has no pages array"))?;
        if n > MAX_RENDER_TREE_PAGES {
            return Err(unavailable("output_limit",
                format!("document has {n} pages; page information is limited to {MAX_RENDER_TREE_PAGES}")));
        }
        self.preview_run(
            &runner,
            &st,
            &[
                "/in/rhwp",
                "export-render-tree",
                "/in/doc.hwpx",
                "-o",
                "/out",
            ],
            cancel,
            &budget,
            deadline,
        )?;
        // Trusted wrapper manifest: no untrusted directory discovery or arbitrary
        // synthesized-path reads. Every name and size is validated before reads.
        let listed =
            tree_inventory(&st.out_dir, n).map_err(|m| unavailable("invalid_output", m))?;
        let mut trees = Vec::with_capacity(n);
        for (name, bytes_len) in listed {
            let charge = bytes_len
                .checked_mul(17)
                .and_then(|n| n.checked_add(65536))
                .ok_or_else(|| unavailable("memory_limit", "tree workspace size overflow"))?;
            budget.reserve(charge)?;
            let bytes = read_out_file_bounded(&st.out_dir, &name, Some(bytes_len), bytes_len)
                .map_err(|m| unavailable("invalid_output", m))?;
            trees.push(
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|e| unavailable("invalid_output", format!("{name}: {e}")))?,
            );
        }
        budget.reserve(doc.nodes.len() as u64 * 512 + doc.state.total_bytes().saturating_mul(2))?;
        if cancel.is_some_and(CancelToken::is_cancelled) {
            return Err(run_error(RunError::Cancelled));
        }
        Ok(join_pages(doc, &trees, &dump))
    }
}

/// Narrow protocol adapter for pinned rhwp's manifestless tree export. The
/// process has finished and its namespace descendants have been terminated.
fn tree_inventory(out_dir: &Path, pages: usize) -> std::result::Result<Vec<(String, u64)>, String> {
    let expected: BTreeSet<String> = (1..=pages)
        .map(|i| format!("render_tree_{i:03}.json"))
        .collect();
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(out_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or("non-UTF8 tree filename")?
            .to_string();
        if !expected.contains(&name) || !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("tree output inventory contains an unexpected or nonregular entry".into());
        }
        found.insert(name);
    }
    if found != expected {
        return Err("tree output inventory is incomplete".into());
    }
    (1..=pages)
        .map(|i| {
            let name = format!("render_tree_{i:03}.json");
            let md = fs::symlink_metadata(out_dir.join(&name)).map_err(|e| e.to_string())?;
            if !md.is_file() || md.len() > MAX_OUT_FILE_BYTES {
                return Err("invalid tree inventory file".into());
            }
            Ok((format!("/out/{name}"), md.len()))
        })
        .collect()
}

fn parse_output_json(out: &ProcOutput, budget: &mut MemoryBudget) -> Result<(Value, u64)> {
    // Includes raw stdout, Value allocations and transient parser workspace.
    let charge = (out.stdout.len() as u64)
        .checked_mul(17)
        .and_then(|n| n.checked_add(out.stderr.len() as u64 + 65536))
        .ok_or_else(|| unavailable("memory_limit", "JSON workspace size overflow"))?;
    budget.reserve(charge)?;
    let value = serde_json::from_slice(&out.stdout).map_err(|e| {
        unavailable(
            "invalid_output",
            format!("renderer output is not JSON: {e}"),
        )
    })?;
    Ok((value, charge))
}

fn bwrap_version(
    bwrap: &Path,
    out_dir: &Path,
    deadline: Instant,
    cancel: Option<&CancelToken>,
    budget: &MemoryBudget,
) -> Option<String> {
    let mut limits = RunLimits::with_timeout_secs(1);
    limits.timeout = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_millis(250));
    if limits.timeout.is_zero() {
        return None;
    }
    limits.stdout_cap = 4096.min((budget.remaining / 4) as usize);
    limits.stderr_cap = limits.stdout_cap;
    let mut cmd = std::process::Command::new(bwrap);
    cmd.arg("--version")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let o = sandbox::run_limited(cmd, out_dir, &limits, cancel).ok()?;
    if !o.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!value.is_empty() && value.len() <= 200 && !value.contains('\n')).then_some(value)
}

#[derive(Debug)]
struct ListedPage {
    page: usize,
    path: String,
    bytes: u64,
}

/// Validate the `export-svg --json` manifest and list its pages in order.
fn parse_manifest(m: &Value) -> std::result::Result<Vec<ListedPage>, String> {
    if m.get("format").and_then(Value::as_str) != Some("svg") {
        return Err("manifest format is not svg".into());
    }
    let count = m
        .get("pageCount")
        .and_then(Value::as_u64)
        .ok_or("manifest has no pageCount")? as usize;
    let pages = m
        .get("pages")
        .and_then(Value::as_array)
        .ok_or("manifest has no pages list")?;
    if pages.len() > count
        || m.get("renderedCount")
            .is_some_and(|v| v.as_u64() != Some(pages.len() as u64))
    {
        return Err(format!(
            "manifest lists {} pages but pageCount is {count}",
            pages.len()
        ));
    }
    let mut out = Vec::with_capacity(pages.len());
    let mut paths = BTreeSet::new();
    let mut previous = None;
    for (i, p) in pages.iter().enumerate() {
        let page = p
            .get("page")
            .and_then(Value::as_u64)
            .filter(|&p| p < count as u64)
            .ok_or_else(|| format!("invalid manifest page {i}"))? as usize;
        if previous.is_some_and(|p| p >= page) {
            return Err("manifest pages out of order".into());
        }
        previous = Some(page);
        let path = p
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("manifest page {i} has no path"))?;
        let bytes = p
            .get("bytes")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("manifest page {i} has no byte size"))?;
        if !paths.insert(path) {
            return Err("duplicate manifest output path".into());
        }
        out.push(ListedPage {
            page,
            path: path.to_string(),
            bytes,
        });
    }
    Ok(out)
}

/// Read one regular file named by the renderer (as `/out/...`) from the host
/// side of `/out`. Absolute-in-sandbox paths only, no `..`, no symlinks at any
/// level, regular files only, size bounded. `None` is only for the explicitly
/// scoped pinned render-tree naming exception; `Some(0)` is exact.
#[cfg(test)]
fn read_out_file(
    out_dir: &Path,
    sandbox_path: &str,
    expect_len: Option<u64>,
) -> std::result::Result<Vec<u8>, String> {
    read_out_file_bounded(out_dir, sandbox_path, expect_len, MAX_OUT_FILE_BYTES)
}

fn read_out_file_bounded(
    out_dir: &Path,
    sandbox_path: &str,
    expect_len: Option<u64>,
    max_bytes: u64,
) -> std::result::Result<Vec<u8>, String> {
    let max_bytes = max_bytes.min(MAX_OUT_FILE_BYTES);
    let rel = sandbox_path
        .strip_prefix("/out/")
        .ok_or_else(|| format!("path {sandbox_path:?} is outside /out"))?;
    if rel.is_empty() || rel.contains('\0') {
        return Err("empty or NUL path".into());
    }
    let mut cur = out_dir.to_path_buf();
    let comps: Vec<&str> = rel.split('/').collect();
    for (i, c) in comps.iter().enumerate() {
        if c.is_empty() || *c == "." || *c == ".." {
            return Err(format!("path {sandbox_path:?} is not canonical"));
        }
        cur.push(c);
        let md = fs::symlink_metadata(&cur).map_err(|e| format!("{sandbox_path}: {e}"))?;
        if md.file_type().is_symlink() {
            return Err(format!("{sandbox_path}: symlinks are rejected"));
        }
        let last = i + 1 == comps.len();
        if last && !md.is_file() {
            return Err(format!("{sandbox_path}: not a regular file"));
        }
        if !last && !md.is_dir() {
            return Err(format!("{sandbox_path}: parent is not a directory"));
        }
        if last {
            if md.len() > max_bytes {
                return Err(format!(
                    "{sandbox_path}: {} bytes exceeds the per-file cap",
                    md.len()
                ));
            }
            if expect_len.is_some_and(|expected| md.len() != expected) {
                return Err(format!(
                    "{sandbox_path}: manifest says {expect_len:?} bytes, file has {}",
                    md.len()
                ));
            }
        }
    }
    let mut f = fs::File::open(&cur).map_err(|e| format!("{sandbox_path}: {e}"))?;
    let mut buf = Vec::new();
    if let Some(size) = expect_len {
        buf.try_reserve_exact(size as usize + 1)
            .map_err(|_| "output buffer allocation failed")?;
    }
    (&mut f)
        .take(max_bytes + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{sandbox_path}: {e}"))?;
    if buf.len() as u64 > max_bytes {
        return Err(format!("{sandbox_path}: exceeds the per-file cap"));
    }
    if expect_len.is_some_and(|n| n != buf.len() as u64) {
        return Err("output size changed during read".into());
    }
    Ok(buf)
}

/// `LAYOUT_*` lines on rhwp's stderr become notes. `page=N` (0-based in
/// rhwp) is converted to a 1-based page when present.
fn layout_notes(stderr: &str) -> Vec<RenderNote> {
    let mut v = Vec::new();
    let mut other = 0usize;
    for line in stderr.lines().take(MAX_NOTES) {
        let line = line.trim();
        if let Some((code, rest)) = line.split_once(':') {
            if code.starts_with("LAYOUT_")
                && code.chars().all(|c| c.is_ascii_uppercase() || c == '_')
            {
                let page = rest
                    .find("page=")
                    .and_then(|i| {
                        let d: String = rest[i + 5..]
                            .chars()
                            .take_while(char::is_ascii_digit)
                            .collect();
                        d.parse::<u32>().ok()
                    })
                    .map(|p| p + 1);
                v.push(note(
                    code,
                    "warning",
                    page,
                    line.chars().take(2048).collect::<String>(),
                ));
                continue;
            }
        }
        if !line.is_empty() {
            other += 1;
        }
    }
    if other > 0 {
        v.push(note(
            "RENDERER_STDERR",
            "info",
            None,
            format!("{other} non-layout line(s) on renderer stderr"),
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_lines_become_notes_with_one_based_pages() {
        let s = "LAYOUT_OVERFLOW: page=0, sec=15, para=15\nLAYOUT_OVERFLOW_DRAW: section=15 pi=15\nhello\n";
        let n = layout_notes(s);
        assert_eq!(n[0].code, "LAYOUT_OVERFLOW");
        assert_eq!(n[0].page, Some(1));
        assert_eq!(n[1].code, "LAYOUT_OVERFLOW_DRAW");
        assert_eq!(n[1].page, None);
        assert_eq!(n[2].code, "RENDERER_STDERR");
    }

    #[test]
    fn duplicate_manifest_paths_are_rejected() {
        let m = json!({"format":"svg","pageCount":2,"pages":[
            {"page":0,"path":"/out/a.svg","bytes":6},
            {"page":1,"path":"/out/a.svg","bytes":6}
        ]});
        assert!(parse_manifest(&m).is_err());
    }

    #[test]
    fn declared_zero_is_an_exact_size() {
        let td = out_dir();
        fs::write(td.path().join("a.svg"), b"<svg/>").unwrap();
        assert!(read_out_file(td.path(), "/out/a.svg", Some(0)).is_err());
    }

    #[test]
    fn manifest_validation() {
        let ok = json!({"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/a.svg","bytes":3}]});
        assert_eq!(parse_manifest(&ok).unwrap().len(), 1);
        for bad in [
            json!({"format":"png","pageCount":0,"pages":[]}),
            json!({"format":"svg","pageCount":0,"pages":[{"page":0,"path":"/out/a.svg","bytes":3}]}),
            json!({"format":"svg","pageCount":1,"pages":[{"page":5,"path":"/out/a.svg","bytes":3}]}),
            json!({"format":"svg","pageCount":1,"pages":[{"page":0,"bytes":3}]}),
        ] {
            assert!(parse_manifest(&bad).is_err(), "{bad}");
        }
    }

    fn out_dir() -> TempDir {
        TempDir::new_in(&std::env::temp_dir(), "hae-readout-").unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn read_out_file_rejects_escapes_and_symlinks() {
        let td = out_dir();
        fs::write(td.path().join("a.svg"), b"<svg/>").unwrap();
        fs::create_dir(td.path().join("sub")).unwrap();
        fs::write(td.path().join("sub").join("b.svg"), b"x").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", td.path().join("link.svg")).unwrap();
        std::os::unix::fs::symlink("/etc", td.path().join("dirlink")).unwrap();
        assert_eq!(
            read_out_file(td.path(), "/out/a.svg", Some(6)).unwrap(),
            b"<svg/>"
        );
        assert!(read_out_file(td.path(), "/out/sub/b.svg", Some(1)).is_ok());
        for bad in [
            "/out/../etc/passwd",
            "/out/./a.svg",
            "/etc/passwd",
            "out/a.svg",
            "/out/link.svg",
            "/out/dirlink/passwd",
            "/out/sub",
            "/out/missing.svg",
            "/out/",
            "/out//a.svg",
        ] {
            assert!(
                read_out_file(td.path(), bad, None).is_err(),
                "{bad} accepted"
            );
        }
        // Manifest size must match the file.
        assert!(read_out_file(td.path(), "/out/a.svg", Some(7)).is_err());
    }
}
