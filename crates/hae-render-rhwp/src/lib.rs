//! hae-render-rhwp: rhwp preview adapter (spec §9·§10, tasks 3-2…3-4).
//!
//! rhwp runs as a separate process inside a bubblewrap sandbox ([`sandbox`]).
//! The adapter hands it a revision-fixed HWPX snapshot and a copy of the
//! binary through a read-only input directory, reads only the files listed in
//! rhwp's JSON manifest from the temporary `/out`, scans every SVG
//! ([`svg_safety`]) and never falls back to another renderer or to an
//! unsandboxed run. Failure is explicit (`RENDERER_UNAVAILABLE` with a
//! machine-readable reason) and never touches document state.

pub mod fonts;
pub mod pages;
pub mod png;
pub mod sandbox;
pub mod svg_safety;

use fonts::{referenced_families, FontFile, FontSet};
use hae_diag::{Code, Diagnostic, EngineError, Limits, Result, Stage};
use hae_model::Document;
use hae_package::sha256_hex;
pub use pages::{join_pages, PageInfo, PageStatus};
use png::{CheckedSvg, PngOptions};
pub use sandbox::CancelToken;
use sandbox::{ProcOutput, RunError, RunLimits, Sandbox, TempDir};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// rhwp commit this engine pins (spec §9).
pub const PINNED_COMMIT: &str = "1a76570e833917d15817415a53c09ad61ab3203f";
/// Default bubblewrap location (explicit; PATH is never searched).
pub const DEFAULT_BWRAP: &str = "/usr/bin/bwrap";

const INPUT_NAME: &str = "doc.hwpx";
const MAX_RENDER_TREE_PAGES: usize = 2000;
const MAX_OUT_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_NOTES: usize = 200;

/// Adapter configuration. Every path is explicit.
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    pub rhwp_path: PathBuf,
    pub bwrap_path: PathBuf,
    pub font_dir: PathBuf,
    /// Parent of the per-run temporary directories.
    pub work_dir: PathBuf,
    /// rhwp commit recorded in metadata (as configured, not derived).
    pub renderer_commit: Option<String>,
    /// Timeout (`renderer_timeout_secs`) and page limit (`max_preview_pages`).
    pub limits: Limits,
    pub stdout_cap: usize,
    pub stderr_cap: usize,
    pub out_cap_bytes: u64,
}

impl AdapterConfig {
    pub fn new(rhwp_path: impl Into<PathBuf>, font_dir: impl Into<PathBuf>) -> Self {
        AdapterConfig {
            rhwp_path: rhwp_path.into(),
            bwrap_path: PathBuf::from(DEFAULT_BWRAP),
            font_dir: font_dir.into(),
            work_dir: std::env::temp_dir(),
            renderer_commit: Some(PINNED_COMMIT.to_string()),
            limits: Limits::default(),
            stdout_cap: 64 * 1024 * 1024,
            stderr_cap: 8 * 1024 * 1024,
            out_cap_bytes: 512 * 1024 * 1024,
        }
    }

    /// Read `HAE_RHWP` (required), `HAE_BWRAP` and `HAE_FONT_DIR`. `None` when
    /// `HAE_RHWP` is unset, i.e. the preview adapter is not configured.
    pub fn from_env() -> Option<Self> {
        let rhwp = std::env::var_os("HAE_RHWP").filter(|v| !v.is_empty())?;
        let font_dir = std::env::var_os("HAE_FONT_DIR")
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/share/fonts".into());
        let mut c = AdapterConfig::new(rhwp, font_dir);
        if let Some(b) = std::env::var_os("HAE_BWRAP").filter(|v| !v.is_empty()) {
            c.bwrap_path = b.into();
        }
        Some(c)
    }

    fn run_limits(&self) -> RunLimits {
        let mut l = RunLimits::with_timeout_secs(self.limits.renderer_timeout_secs);
        l.stdout_cap = self.stdout_cap;
        l.stderr_cap = self.stderr_cap;
        l.out_cap_bytes = self.out_cap_bytes;
        l
    }
}

fn unavailable(reason: &str, msg: impl Into<String>) -> EngineError {
    EngineError::new(Code::RendererUnavailable, Stage::Render, msg).with_reason(reason)
}

fn run_error(e: RunError) -> EngineError {
    unavailable(e.reason(), e.to_string())
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

#[derive(Default)]
struct State {
    sandbox_verified: bool,
    fonts: Option<Arc<FontSet>>,
}

/// The rhwp adapter. Cheap to share behind an `Arc`.
pub struct RhwpAdapter {
    cfg: AdapterConfig,
    state: Mutex<State>,
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
        let mut st = self.state.lock().unwrap();
        if let Some(f) = &st.fonts {
            if f.is_current() {
                return Ok(f.clone());
            }
        }
        let f = Arc::new(FontSet::scan(&self.cfg.font_dir).map_err(|e| {
            unavailable(
                "font_dir_unreadable",
                format!("font directory {}: {e}", self.cfg.font_dir.display()),
            )
        })?);
        st.fonts = Some(f.clone());
        Ok(f)
    }

    /// Verify the sandbox once (isolation self-test). Any failure — missing
    /// bwrap, failed probe, unusable fonts mount — means `sandbox_unverified`.
    fn sandbox(&self) -> Result<Sandbox> {
        let sb = Sandbox::new(&self.cfg.bwrap_path, &self.cfg.font_dir);
        let mut st = self.state.lock().unwrap();
        if !st.sandbox_verified {
            if !self.cfg.font_dir.is_dir() {
                return Err(unavailable(
                    "sandbox_unverified",
                    format!(
                        "font directory {} is not a directory",
                        self.cfg.font_dir.display()
                    ),
                ));
            }
            sb.self_test(&self.cfg.work_dir).map_err(|m| {
                unavailable(
                    "sandbox_unverified",
                    format!("sandbox isolation could not be verified: {m}"),
                )
            })?;
            st.sandbox_verified = true;
        }
        Ok(sb)
    }

    fn stage(&self, hwpx: &[u8]) -> Result<Staged> {
        let rhwp = fs::read(&self.cfg.rhwp_path).map_err(|e| {
            unavailable(
                "renderer_missing",
                format!("rhwp binary {}: {e}", self.cfg.rhwp_path.display()),
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
            fs::DirBuilder::new()
                .mode(0o700)
                .create(d)
                .map_err(|e| io("mkdir", e))?;
        }
        let bin = in_dir.join("rhwp");
        fs::write(&bin, &rhwp).map_err(|e| io("copy renderer", e))?;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).map_err(|e| io("chmod", e))?;
        fs::write(in_dir.join(INPUT_NAME), hwpx).map_err(|e| io("write snapshot", e))?;
        Ok(Staged {
            rhwp_sha256: sha256_hex(&rhwp),
            input_sha256: sha256_hex(hwpx),
            in_dir,
            out_dir,
            _td: td,
        })
    }

    fn run(
        &self,
        sb: &Sandbox,
        st: &Staged,
        argv: &[&str],
        cancel: Option<&CancelToken>,
    ) -> Result<ProcOutput> {
        let out = sb
            .run(
                &st.in_dir,
                &st.out_dir,
                argv,
                &self.cfg.run_limits(),
                cancel,
            )
            .map_err(run_error)?;
        if !out.success() {
            let tail = String::from_utf8_lossy(&out.stderr);
            let tail: String = tail
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
            .with_detail(json!({ "stderrTail": tail })));
        }
        Ok(out)
    }

    fn renderer_version(
        &self,
        sb: &Sandbox,
        st: &Staged,
        cancel: Option<&CancelToken>,
    ) -> Result<String> {
        let out = self.run(sb, st, &["/in/rhwp", "--version"], cancel)?;
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !v.starts_with("rhwp") || v.len() > 200 || v.contains('\n') {
            return Err(unavailable(
                "invalid_output",
                format!("unrecognised renderer version output: {v:?}"),
            ));
        }
        Ok(v)
    }

    /// Render a revision-fixed snapshot to SVG (and optionally PNG) pages.
    pub fn preview(
        &self,
        req: &RenderRequest<'_>,
        cancel: Option<&CancelToken>,
    ) -> Result<PreviewOutput> {
        let started = std::time::Instant::now();
        if self.cfg.rhwp_path.as_os_str().is_empty() {
            return Err(unavailable(
                "renderer_not_configured",
                "no rhwp binary configured",
            ));
        }
        let sb = self.sandbox()?;
        let fonts = self.fonts()?;
        let st = self.stage(req.hwpx)?;
        let version = self.renderer_version(&sb, &st, cancel)?;
        let out = self.run(
            &sb,
            &st,
            &[
                "/in/rhwp",
                "export-svg",
                "/in/doc.hwpx",
                "-o",
                "/out",
                "--json",
            ],
            cancel,
        )?;

        let manifest: Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| unavailable("invalid_output", format!("manifest is not JSON: {e}")))?;
        let listed = parse_manifest(&manifest).map_err(|m| unavailable("invalid_output", m))?;
        let total = listed.len();
        let max_pages = self.cfg.limits.max_preview_pages as usize;
        let limit_hit = total > max_pages;
        let rendered_count = total.min(max_pages);

        let mut notes = Vec::new();
        let mut diagnostics = Vec::new();
        notes.extend(layout_notes(&String::from_utf8_lossy(&out.stderr)));
        if manifest.get("untrustedContent").and_then(Value::as_bool) == Some(true) {
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
        {
            notes.push(note(
                "LAYOUT_OVERFLOW_CELL_LINES",
                "warning",
                None,
                "rhwp reported table cell lines that overflow their cells",
            ));
        }
        if limit_hit {
            diagnostics.push(
                Diagnostic::error(
                    Code::PreviewPageLimit,
                    Stage::Render,
                    format!("document has {total} pages; only the first {max_pages} were rendered"),
                )
                .with_detail(json!({ "totalPages": total, "maxPages": max_pages })),
            );
        }

        // Which pages to return.
        let wanted: Vec<usize> = match req.pages {
            None => (0..rendered_count).collect(),
            Some(list) => {
                let mut seen = BTreeSet::new();
                for &p in list {
                    if p >= 1 && (p as usize) <= rendered_count {
                        seen.insert(p as usize - 1);
                    } else {
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
        let missing_requested = req.pages.is_some_and(|l| l.len() > wanted.len());

        let mut pages_out = Vec::new();
        let mut dropped_unsafe = 0usize;
        let mut families = BTreeSet::new();
        for idx in wanted {
            let lp = &listed[idx];
            let bytes = read_out_file(&st.out_dir, &lp.path, lp.bytes)
                .map_err(|m| unavailable("invalid_output", m))?;
            let chk = svg_safety::check_svg(&bytes);
            if !chk.is_safe() {
                dropped_unsafe += 1;
                let summary: Vec<String> = chk
                    .issues
                    .iter()
                    .take(5)
                    .map(|i| format!("{:?}: {}", i.kind, i.detail))
                    .collect();
                notes.push(note(
                    "SVG_UNSAFE",
                    "error",
                    Some(idx as u32 + 1),
                    format!(
                        "page withheld, unsafe SVG ({} issues): {}",
                        chk.issues.len(),
                        summary.join("; ")
                    ),
                ));
                continue;
            }
            if let Ok(text) = std::str::from_utf8(&bytes) {
                families.extend(referenced_families(text));
            }
            let mut page = PreviewPage {
                page: idx as u32 + 1,
                svg_sha256: sha256_hex(&bytes),
                svg: bytes,
                png: None,
                png_sha256: None,
            };
            if req.want_png {
                match CheckedSvg::new(&page.svg)
                    .map_err(|_| "page SVG failed the safety scan".to_string())
                    .and_then(|c| {
                        png::rasterize_svg(c, &fonts, &req.png).map_err(|e| e.to_string())
                    }) {
                    Ok(p) => {
                        page.png_sha256 = Some(sha256_hex(&p));
                        page.png = Some(p);
                    }
                    Err(m) => notes.push(note("PNG_FAILED", "warning", Some(page.page), m)),
                }
            }
            pages_out.push(page);
        }

        if !families.is_empty() {
            let available = fonts.families();
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
        } else if limit_hit || dropped_unsafe > 0 || missing_requested {
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
        let bwrap_version = bwrap_version(&self.cfg.bwrap_path);
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
            }),
            sandbox: json!({ "tool": "bubblewrap", "version": bwrap_version, "selfTest": "passed" }),
            fonts: json!({
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
        let hwpx = doc.to_hwpx()?;
        let sb = self.sandbox()?;
        let st = self.stage(&hwpx)?;
        let dump_out = self.run(
            &sb,
            &st,
            &["/in/rhwp", "dump-pages", "/in/doc.hwpx", "--json"],
            cancel,
        )?;
        let dump: Value = serde_json::from_slice(&dump_out.stdout).map_err(|e| {
            unavailable(
                "invalid_output",
                format!("dump-pages output is not JSON: {e}"),
            )
        })?;
        let n = dump
            .get("pages")
            .and_then(Value::as_array)
            .map(Vec::len)
            .ok_or_else(|| unavailable("invalid_output", "dump-pages output has no pages array"))?;
        if n > MAX_RENDER_TREE_PAGES {
            return Err(unavailable(
                "output_limit",
                format!("document has {n} pages; page information is limited to {MAX_RENDER_TREE_PAGES}"),
            ));
        }
        self.run(
            &sb,
            &st,
            &[
                "/in/rhwp",
                "export-render-tree",
                "/in/doc.hwpx",
                "-o",
                "/out",
            ],
            cancel,
        )?;
        let mut trees = Vec::with_capacity(n);
        for i in 1..=n {
            let name = format!("/out/render_tree_{i:03}.json");
            let bytes = read_out_file(&st.out_dir, &name, 0)
                .map_err(|m| unavailable("invalid_output", m))?;
            trees.push(
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|e| unavailable("invalid_output", format!("{name}: {e}")))?,
            );
        }
        Ok(join_pages(doc, &trees, &dump))
    }
}

fn bwrap_version(bwrap: &Path) -> Option<String> {
    let o = std::process::Command::new(bwrap)
        .arg("--version")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

#[derive(Debug)]
struct ListedPage {
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
    if pages.len() != count {
        return Err(format!(
            "manifest lists {} pages but pageCount is {count}",
            pages.len()
        ));
    }
    let mut out = Vec::with_capacity(count);
    for (i, p) in pages.iter().enumerate() {
        if p.get("page").and_then(Value::as_u64) != Some(i as u64) {
            return Err(format!("manifest page entry {i} is out of order"));
        }
        let path = p
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("manifest page {i} has no path"))?;
        let bytes = p
            .get("bytes")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("manifest page {i} has no byte size"))?;
        out.push(ListedPage {
            path: path.to_string(),
            bytes,
        });
    }
    Ok(out)
}

/// Read one regular file named by the renderer (as `/out/...`) from the host
/// side of `/out`. Absolute-in-sandbox paths only, no `..`, no symlinks at any
/// level, regular files only, size bounded. `expect_len == 0` skips the exact
/// size check (render-tree files have no manifest).
fn read_out_file(
    out_dir: &Path,
    sandbox_path: &str,
    expect_len: u64,
) -> std::result::Result<Vec<u8>, String> {
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
            if md.len() > MAX_OUT_FILE_BYTES {
                return Err(format!(
                    "{sandbox_path}: {} bytes exceeds the per-file cap",
                    md.len()
                ));
            }
            if expect_len != 0 && md.len() != expect_len {
                return Err(format!(
                    "{sandbox_path}: manifest says {expect_len} bytes, file has {}",
                    md.len()
                ));
            }
        }
    }
    let mut f = fs::File::open(&cur).map_err(|e| format!("{sandbox_path}: {e}"))?;
    let mut buf = Vec::new();
    (&mut f)
        .take(MAX_OUT_FILE_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{sandbox_path}: {e}"))?;
    if buf.len() as u64 > MAX_OUT_FILE_BYTES {
        return Err(format!("{sandbox_path}: exceeds the per-file cap"));
    }
    Ok(buf)
}

/// `LAYOUT_*` lines on rhwp's stderr become notes. `page=N` (0-based in
/// rhwp) is converted to a 1-based page when present.
fn layout_notes(stderr: &str) -> Vec<RenderNote> {
    let mut v = Vec::new();
    let mut other = 0usize;
    for line in stderr.lines() {
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
                v.push(note(code, "warning", page, line));
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
    fn manifest_validation() {
        let ok = json!({"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/a.svg","bytes":3}]});
        assert_eq!(parse_manifest(&ok).unwrap().len(), 1);
        for bad in [
            json!({"format":"png","pageCount":0,"pages":[]}),
            json!({"format":"svg","pageCount":2,"pages":[{"page":0,"path":"/out/a.svg","bytes":3}]}),
            json!({"format":"svg","pageCount":1,"pages":[{"page":5,"path":"/out/a.svg","bytes":3}]}),
            json!({"format":"svg","pageCount":1,"pages":[{"page":0,"bytes":3}]}),
        ] {
            assert!(parse_manifest(&bad).is_err(), "{bad}");
        }
    }

    fn out_dir() -> TempDir {
        TempDir::new_in(&std::env::temp_dir(), "hae-readout-").unwrap()
    }

    #[test]
    fn read_out_file_rejects_escapes_and_symlinks() {
        let td = out_dir();
        fs::write(td.path().join("a.svg"), b"<svg/>").unwrap();
        fs::create_dir(td.path().join("sub")).unwrap();
        fs::write(td.path().join("sub").join("b.svg"), b"x").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", td.path().join("link.svg")).unwrap();
        std::os::unix::fs::symlink("/etc", td.path().join("dirlink")).unwrap();
        assert_eq!(
            read_out_file(td.path(), "/out/a.svg", 6).unwrap(),
            b"<svg/>"
        );
        assert!(read_out_file(td.path(), "/out/sub/b.svg", 1).is_ok());
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
            assert!(read_out_file(td.path(), bad, 0).is_err(), "{bad} accepted");
        }
        // Manifest size must match the file.
        assert!(read_out_file(td.path(), "/out/a.svg", 7).is_err());
    }
}
