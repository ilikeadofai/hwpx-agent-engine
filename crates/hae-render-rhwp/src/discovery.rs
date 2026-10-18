//! Preview tool discovery (task 0.4-3).
//!
//! Every tool the preview adapter needs has one rule: an explicit environment
//! variable (`HAE_RHWP`, `HAE_BWRAP`, `HAE_FONT_DIR`, `HAE_RASTER`) always wins
//! unchanged, and only when it is unset does the adapter look in a fixed,
//! documented order. Discovery decides *which* path is used; it never relaxes
//! the sandbox contract — a missing or unverifiable sandbox still refuses to
//! start the renderer.
//!
//! Every search records the candidates it examined (path, rule, verdict) so a
//! failure can say what to do ("set `HAE_RHWP` or put the executable beside
//! this application") instead of only that something is missing.
//!
//! The candidate builders and the environment parser are pure functions over
//! injected inputs, so the priority order is unit-testable without touching
//! the process environment.

use hae_diag::EngineError;
use serde::Serialize;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// `--help` text for every interface, kept next to the discovery rules so the
/// documented order cannot drift from the implemented one.
pub const RENDERER_ENV_HELP: &str = "\
Preview environment (used by document.preview):
  HAE_RHWP      Path to the sandboxed rhwp renderer executable. When unset it is
                looked up beside this application, in ../bin, in an in-tree
                spikes/vendor/rhwp-src/target/{release,debug} build, then on PATH
  HAE_BWRAP     Path to bubblewrap (default /usr/bin/bwrap, then PATH)
  HAE_FONT_DIR  Font directory mounted for rendering, used exactly as given (an
                invalid or over-limit directory is an error, never replaced). When
                unset: <app dir>/fonts, per-user font dirs, then the first system
                font dir within the font limits (/usr/share/fonts,
                /usr/local/share/fonts, ...); if none fits, one bounded
                subdirectory of an oversized system dir, reported as such
  HAE_RASTER    PNG worker executable (default hae-raster next to this application)
  HAE_RENDERER_DISCOVERY=0  Disable the search; only the variables above are used";

/// How many parent directories a development-tree search walks up from a root.
/// A checkout root is reached from `target/<profile>/deps` in three steps; the
/// small bound keeps an accidental match in an unrelated ancestor unlikely.
const MAX_ROOT_WALK: usize = 4;

/// Relative path of the in-tree renderer build below a checkout root.
const DEV_TREE: &str = "spikes/vendor/rhwp-src/target";

/// Set this to `0` to disable discovery entirely: only explicit environment
/// variables are used, and an unset variable means "not configured".
pub const DISCOVERY_ENV: &str = "HAE_RENDERER_DISCOVERY";

/// Which rule produced a candidate path. Stable strings on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Explicit environment variable (never probed, never replaced).
    Env,
    /// Set directly on the configuration object by the caller.
    Explicit,
    /// Next to the running executable (bundled layout).
    ExeDir,
    /// `<executable dir>/../bin`, the usual install prefix layout.
    ExeParentBin,
    /// `<root>/spikes/vendor/rhwp-src/target/release`.
    DevTreeRelease,
    /// `<root>/spikes/vendor/rhwp-src/target/debug`.
    DevTreeDebug,
    /// Built-in default path.
    Default,
    /// A `PATH` entry.
    Path,
    /// A font directory shipped with the application (`<exe dir>/fonts`).
    AppFontDir,
    /// A well-known system font directory.
    SystemFontDir,
    /// An immediate subdirectory of a system font directory that is itself
    /// too large to use whole (bounded fallback, see [`select_font_dir`]).
    SystemFontSubdir,
    /// A per-user font directory below the home directory.
    UserFontDir,
    /// A platform font directory (`%WINDIR%\Fonts`, `%LOCALAPPDATA%\...`).
    PlatformFontDir,
}

impl Origin {
    /// Stable machine string (also the `origin` wire field).
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Env => "env",
            Origin::Explicit => "explicit",
            Origin::ExeDir => "exeDir",
            Origin::ExeParentBin => "exeParentBin",
            Origin::DevTreeRelease => "devTreeRelease",
            Origin::DevTreeDebug => "devTreeDebug",
            Origin::Default => "default",
            Origin::Path => "path",
            Origin::AppFontDir => "appFontDir",
            Origin::SystemFontDir => "systemFontDir",
            Origin::SystemFontSubdir => "systemFontSubdir",
            Origin::UserFontDir => "userFontDir",
            Origin::PlatformFontDir => "platformFontDir",
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One path a search may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub origin: Origin,
}

impl Candidate {
    fn new(path: impl Into<PathBuf>, origin: Origin) -> Self {
        Candidate {
            path: path.into(),
            origin,
        }
    }
}

/// What `probe` decided about one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Accepted.
    Found,
    /// Nothing at that path.
    Missing,
    /// Present but not a regular executable file.
    NotExecutable,
    /// Present but not a directory.
    NotDirectory,
    /// A directory the preview cannot use as its font set: it exceeds the
    /// font limits or holds no font files (the detail says which).
    Unusable,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Found => "found",
            Verdict::Missing => "missing",
            Verdict::NotExecutable => "notExecutable",
            Verdict::NotDirectory => "notDirectory",
            Verdict::Unusable => "unusable",
        }
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a search accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// A regular file with an execute bit (Unix); a regular file elsewhere.
    Executable,
    /// A directory.
    Directory,
    /// A directory that also passes the bounded font preflight
    /// ([`crate::fonts::inspect_font_dir`]) and holds at least one font file.
    FontDirectory,
}

/// One examined candidate with the verdict, kept for diagnostics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Examined {
    pub path: PathBuf,
    pub origin: Origin,
    pub verdict: Verdict,
    /// Why the verdict is what it is, when the verdict alone is not enough
    /// (font limits exceeded, font count and size of an accepted directory).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl std::fmt::Display for Examined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{}]: {}",
            self.path.display(),
            self.origin,
            self.verdict
        )?;
        match &self.detail {
            Some(d) => write!(f, " ({d})"),
            None => Ok(()),
        }
    }
}

/// The outcome of probing an ordered candidate list.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    /// Accepted path, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Rule that produced the accepted path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Origin>,
    /// Candidates examined in order, up to and including the accepted one.
    pub searched: Vec<Examined>,
    /// A bounded-fallback choice the user must know about: what was selected
    /// and what was left out (font directories only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ProbeOutcome {
    /// An explicit value (environment or direct configuration). Explicit
    /// values are used as given: they are never probed and never replaced, so
    /// a typo stays visible as `renderer_missing` instead of silently
    /// selecting a different renderer.
    pub fn explicit(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        ProbeOutcome {
            searched: vec![Examined {
                path: path.clone(),
                origin: Origin::Env,
                verdict: Verdict::Found,
                detail: None,
            }],
            path: Some(path),
            source: Some(Origin::Env),
            note: None,
        }
    }

    pub fn is_found(&self) -> bool {
        self.path.is_some()
    }

    /// Rule string for diagnostics: `env`, `exeDir`, … or `unset`.
    pub fn source_str(&self) -> &'static str {
        self.source.map_or("unset", Origin::as_str)
    }

    /// `path [rule]; path [rule]; …`, bounded so a message cannot grow with
    /// an unbounded `PATH`.
    pub fn searched_str(&self, limit: usize) -> String {
        self.searched
            .iter()
            .take(limit)
            .map(Examined::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Injected search inputs, so ordering is testable without the environment.
#[derive(Debug, Clone, Default)]
pub struct SearchPaths {
    /// Directory of the running executable, `deps/` stripped (`None` when the
    /// executable path is unknown).
    pub exe_dir: Option<PathBuf>,
    /// `PATH` entries in order.
    pub path_dirs: Vec<PathBuf>,
    /// Roots walked upward looking for a development tree.
    pub roots: Vec<PathBuf>,
    /// Home directory, for per-user font locations.
    pub home: Option<PathBuf>,
    /// Platform font directories (Windows only).
    pub platform_font_dirs: Vec<PathBuf>,
}

impl SearchPaths {
    /// Read the real process: executable location, `PATH`, working directory,
    /// home and platform font directories.
    pub fn from_process() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
            .map(|d| strip_deps_dir(&d));
        let path_dirs = std::env::var_os("PATH")
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_default();
        let mut roots = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd);
        }
        if let Some(d) = &exe_dir {
            if !roots.contains(d) {
                roots.push(d.clone());
            }
        }
        SearchPaths {
            exe_dir,
            path_dirs,
            roots,
            home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            platform_font_dirs: platform_font_dirs(),
        }
    }
}

/// `target/<profile>/deps` holds test executables; the sibling tools live one
/// level up (the same rule `worker::default_worker_path` applies).
fn strip_deps_dir(dir: &Path) -> PathBuf {
    if dir.file_name().is_some_and(|n| n == "deps") {
        return dir.parent().map(Path::to_path_buf).unwrap_or_default();
    }
    dir.to_path_buf()
}

/// Platform font directories from the environment (`cfg(windows)` only).
fn platform_font_dirs() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut v = Vec::new();
        if let Some(w) = std::env::var_os("WINDIR").filter(|v| !v.is_empty()) {
            v.push(PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            v.push(
                PathBuf::from(l)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
        v
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Explicit environment values. `None` means unset or empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    pub rhwp: Option<PathBuf>,
    pub bwrap: Option<PathBuf>,
    pub raster: Option<PathBuf>,
    pub font_dir: Option<PathBuf>,
    /// Discovery is on unless `HAE_RENDERER_DISCOVERY` says otherwise.
    pub discovery: bool,
}

impl Overrides {
    pub fn from_process() -> Self {
        Overrides::from_values(|k| std::env::var_os(k))
    }

    /// Parse explicit values from any source, without touching the process
    /// environment (so a test can set them deterministically).
    pub fn from_values(get: impl Fn(&str) -> Option<OsString>) -> Self {
        let path = |key: &str| get(key).filter(|v| !v.is_empty()).map(PathBuf::from);
        Overrides {
            rhwp: path("HAE_RHWP"),
            bwrap: path("HAE_BWRAP"),
            raster: path("HAE_RASTER"),
            font_dir: path("HAE_FONT_DIR"),
            discovery: discovery_enabled(get(DISCOVERY_ENV).as_deref()),
        }
    }
}

/// `0`, `false`, `off` and `no` disable discovery; anything else (including
/// unset) leaves it on.
pub fn discovery_enabled(value: Option<&OsStr>) -> bool {
    !matches!(
        value.and_then(OsStr::to_str).map(str::trim),
        Some(v) if v.eq_ignore_ascii_case("0")
            || v.eq_ignore_ascii_case("false")
            || v.eq_ignore_ascii_case("off")
            || v.eq_ignore_ascii_case("no")
    )
}

/// The platform file name of a tool (`rhwp` → `rhwp.exe` on Windows).
pub fn tool_name(base: &str) -> String {
    format!("{base}{}", std::env::consts::EXE_SUFFIX)
}

/// `rhwp` candidates: (a) beside the executable, (b) the parent's `bin/`,
/// (c) the development tree under any search root, (d) `PATH`.
pub fn rhwp_candidates(sp: &SearchPaths) -> Vec<Candidate> {
    let name = tool_name("rhwp");
    let mut v = Vec::new();
    if let Some(dir) = &sp.exe_dir {
        v.push(Candidate::new(dir.join(&name), Origin::ExeDir));
        if let Some(parent) = dir.parent() {
            v.push(Candidate::new(
                parent.join("bin").join(&name),
                Origin::ExeParentBin,
            ));
        }
    }
    for root in &sp.roots {
        let mut dir = Some(root.as_path());
        for _ in 0..=MAX_ROOT_WALK {
            let Some(d) = dir else { break };
            v.push(Candidate::new(
                d.join(DEV_TREE).join("release").join(&name),
                Origin::DevTreeRelease,
            ));
            v.push(Candidate::new(
                d.join(DEV_TREE).join("debug").join(&name),
                Origin::DevTreeDebug,
            ));
            dir = d.parent();
        }
    }
    for dir in &sp.path_dirs {
        v.push(Candidate::new(dir.join(&name), Origin::Path));
    }
    dedup(v)
}

/// `bwrap` candidates: the documented default first, then `PATH`.
pub fn bwrap_candidates(sp: &SearchPaths) -> Vec<Candidate> {
    let mut v = vec![
        Candidate::new("/usr/bin/bwrap", Origin::Default),
        Candidate::new("/bin/bwrap", Origin::Default),
        Candidate::new("/usr/local/bin/bwrap", Origin::Default),
    ];
    for dir in &sp.path_dirs {
        v.push(Candidate::new(dir.join("bwrap"), Origin::Path));
    }
    dedup(v)
}

/// `hae-raster` candidates: beside the executable, then the parent's `bin/`.
/// Both processes come from one build; `PATH` is never searched.
pub fn raster_candidates(sp: &SearchPaths) -> Vec<Candidate> {
    let name = tool_name("hae-raster");
    let mut v = Vec::new();
    if let Some(dir) = &sp.exe_dir {
        v.push(Candidate::new(dir.join(&name), Origin::ExeDir));
        if let Some(parent) = dir.parent() {
            v.push(Candidate::new(
                parent.join("bin").join(&name),
                Origin::ExeParentBin,
            ));
        }
    }
    dedup(v)
}

/// Font directory candidates in priority order: the application's own
/// `fonts/` directory, per-user directories, the whole system directories
/// (`/usr/share/fonts` first, the previous default), then platform ones.
///
/// Each candidate is used whole or not at all. A candidate that exceeds the
/// font limits is skipped with its reason recorded; only when no whole
/// directory is usable does [`select_font_dir`] fall back to a bounded
/// subdirectory of an oversized system directory.
pub fn font_dir_candidates(sp: &SearchPaths) -> Vec<Candidate> {
    let mut v = Vec::new();
    if let Some(dir) = &sp.exe_dir {
        v.push(Candidate::new(dir.join("fonts"), Origin::AppFontDir));
    }
    if let Some(home) = &sp.home {
        v.push(Candidate::new(
            home.join(".local").join("share").join("fonts"),
            Origin::UserFontDir,
        ));
        v.push(Candidate::new(home.join(".fonts"), Origin::UserFontDir));
    }
    #[cfg(unix)]
    {
        for d in [
            "/usr/share/fonts",
            "/usr/local/share/fonts",
            "/usr/share/fonts/truetype",
        ] {
            v.push(Candidate::new(d, Origin::SystemFontDir));
        }
    }
    for d in &sp.platform_font_dirs {
        v.push(Candidate::new(d.clone(), Origin::PlatformFontDir));
    }
    dedup(v)
}

/// Most subdirectories of one oversized system font directory the fallback
/// examines (sorted by name), so the search stays bounded.
const MAX_FONT_SUBDIRS: usize = 64;
/// Most omitted subdirectories named in the fallback note.
const MAX_NOTED_OMITTED: usize = 8;

/// Choose the font directory when `HAE_FONT_DIR` is unset.
///
/// 1. The first candidate that passes the bounded font preflight whole.
/// 2. Otherwise, a deterministic bounded subset: the immediate subdirectories
///    (sorted by name, at most [`MAX_FONT_SUBDIRS`] per directory) of every
///    system font directory that was rejected for exceeding the limits. The
///    first usable one with a Korean-named font wins, else the first usable
///    one. The outcome's `note` records the selection and every omission, so
///    the narrower font set is never presented as full coverage.
///
/// The limits are never relaxed.
pub fn select_font_dir(cands: &[Candidate]) -> ProbeOutcome {
    let mut out = probe(cands, Expect::FontDirectory);
    if out.is_found() {
        return out;
    }
    let oversized: Vec<PathBuf> = out
        .searched
        .iter()
        .filter(|e| e.origin == Origin::SystemFontDir && e.verdict == Verdict::Unusable)
        .filter(|e| {
            e.detail
                .as_deref()
                .is_some_and(|d| !d.starts_with("no font files"))
        })
        .map(|e| e.path.clone())
        .collect();
    let mut usable: Vec<(Candidate, crate::fonts::FontScanReport)> = Vec::new();
    let mut omitted: Vec<String> = Vec::new();
    for root in &oversized {
        for sub in font_subdirs(root) {
            let report = crate::fonts::inspect_font_dir(&sub);
            let (verdict, detail) = font_verdict(&report);
            out.searched.push(Examined {
                path: sub.clone(),
                origin: Origin::SystemFontSubdir,
                verdict,
                detail: Some(detail),
            });
            if verdict == Verdict::Found {
                usable.push((Candidate::new(sub, Origin::SystemFontSubdir), report));
            } else {
                omitted.push(sub.display().to_string());
            }
        }
    }
    let chosen = usable
        .iter()
        .position(|(_, r)| r.korean_filename_matches > 0)
        .or(if usable.is_empty() { None } else { Some(0) });
    let Some(i) = chosen else {
        return out;
    };
    let (cand, report) = &usable[i];
    let mut left: Vec<String> = usable
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != i)
        .map(|(_, (c, _))| c.path.display().to_string())
        .collect();
    left.extend(omitted);
    let shown: Vec<&str> = left
        .iter()
        .take(MAX_NOTED_OMITTED)
        .map(String::as_str)
        .collect();
    out.note = Some(format!(
        "no font directory is usable whole ({}); selected {} ({} fonts, {} bytes, {} Korean-named) \
         as a bounded subset: {} other subdirectories were omitted{}{}. Set HAE_FONT_DIR to choose \
         the fonts explicitly.",
        out.searched
            .iter()
            .filter(|e| e.origin != Origin::SystemFontSubdir && e.verdict == Verdict::Unusable)
            .map(|e| e.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        cand.path.display(),
        report.file_count,
        report.total_bytes,
        report.korean_filename_matches,
        left.len(),
        if shown.is_empty() { "" } else { ": " },
        shown.join(", "),
    ));
    out.path = Some(cand.path.clone());
    out.source = Some(cand.origin);
    out
}

/// Immediate, non-symlink subdirectories of `root`, sorted by name.
fn font_subdirs(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.truncate(MAX_FONT_SUBDIRS);
    dirs
}

/// Verdict and human-readable detail for a bounded font preflight.
fn font_verdict(report: &crate::fonts::FontScanReport) -> (Verdict, String) {
    match &report.error {
        Some(e) => (
            Verdict::Unusable,
            format!(
                "{e}; {} font files, {} bytes{}",
                report.file_count,
                report.total_bytes,
                if report.measurements_complete {
                    ""
                } else {
                    " (lower bounds)"
                }
            ),
        ),
        None if report.file_count == 0 => (Verdict::Unusable, "no font files".to_string()),
        None => (
            Verdict::Found,
            format!(
                "{} font files, {} bytes",
                report.file_count, report.total_bytes
            ),
        ),
    }
}

/// Drop repeated paths, keeping the first (highest priority) rule.
fn dedup(v: Vec<Candidate>) -> Vec<Candidate> {
    let mut seen = std::collections::HashSet::new();
    v.into_iter()
        .filter(|c| seen.insert(c.path.clone()))
        .collect()
}

/// Why a discovery failure happened, with everything needed to fix it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryFailure {
    /// Tool that could not be located (`rhwp`).
    pub tool: &'static str,
    /// One line: what is missing and what to do.
    pub message: String,
    /// Candidates examined, in order.
    pub searched: Vec<Examined>,
}

impl DiscoveryFailure {
    /// `RENDERER_UNAVAILABLE(renderer_not_configured)` carrying the search
    /// result as `detail.searched`.
    pub fn to_error(&self) -> EngineError {
        crate::unavailable("renderer_not_configured", self.message.clone()).with_detail(
            serde_json::json!({
                "tool": self.tool,
                "searched": self.searched.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            }),
        )
    }
}

/// Resolved preview tools and where each came from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTools {
    pub rhwp: ProbeOutcome,
    pub bwrap: ProbeOutcome,
    pub font_dir: ProbeOutcome,
    pub raster: ProbeOutcome,
    /// Discovery was disabled by `HAE_RENDERER_DISCOVERY`.
    pub discovery_disabled: bool,
}

impl ResolvedTools {
    /// Record explicitly configured paths without probing them (used when a
    /// caller builds `AdapterConfig` directly).
    pub fn all_explicit(rhwp: &Path, bwrap: &Path, font_dir: &Path, raster: &Path) -> Self {
        let explicit = |p: &Path| ProbeOutcome {
            path: Some(p.to_path_buf()),
            source: Some(Origin::Explicit),
            searched: Vec::new(),
            note: None,
        };
        ResolvedTools {
            rhwp: explicit(rhwp),
            bwrap: explicit(bwrap),
            font_dir: explicit(font_dir),
            raster: explicit(raster),
            discovery_disabled: false,
        }
    }

    /// Resolve every tool: explicit variables first, then the documented
    /// search order. `None` for `rhwp` means the adapter is unconfigured.
    pub fn resolve(ov: &Overrides, sp: &SearchPaths) -> Self {
        let pick =
            |explicit: &Option<PathBuf>, cands: Vec<Candidate>, expect: Expect| match explicit {
                Some(p) => ProbeOutcome::explicit(p.clone()),
                None if ov.discovery => probe(&cands, expect),
                None => ProbeOutcome::default(),
            };
        ResolvedTools {
            rhwp: pick(&ov.rhwp, rhwp_candidates(sp), Expect::Executable),
            bwrap: pick(&ov.bwrap, bwrap_candidates(sp), Expect::Executable),
            font_dir: match &ov.font_dir {
                Some(p) => ProbeOutcome::explicit(p.clone()),
                None if ov.discovery => select_font_dir(&font_dir_candidates(sp)),
                None => ProbeOutcome::default(),
            },
            raster: pick(&ov.raster, raster_candidates(sp), Expect::Executable),
            discovery_disabled: !ov.discovery,
        }
    }

    /// The failure that keeps the adapter unconfigured, or `None`.
    pub fn rhwp_failure(&self) -> Option<DiscoveryFailure> {
        if self.rhwp.is_found() {
            return None;
        }
        let mut message = String::from(
            "rhwp renderer executable not found: set HAE_RHWP to its path, or place it beside \
             this application (or on PATH)",
        );
        if self.discovery_disabled {
            message.push_str(&format!(
                " (discovery is disabled by {DISCOVERY_ENV}; only HAE_RHWP is used)"
            ));
        } else if !self.rhwp.searched.is_empty() {
            message.push_str(&format!("; searched: {}", self.rhwp.searched_str(12)));
        }
        Some(DiscoveryFailure {
            tool: "rhwp",
            message,
            searched: self.rhwp.searched.clone(),
        })
    }

    /// Guidance for a `sandbox_unverified` failure: which `bwrap` was used and
    /// what a missing sandbox needs.
    pub fn bwrap_note(&self) -> String {
        format!(
            "bwrap {} [{}{}]",
            self.bwrap
                .path
                .as_ref()
                .map_or_else(|| "<unset>".into(), |p| p.display().to_string()),
            self.bwrap.source_str(),
            if self.bwrap.searched.is_empty() {
                String::new()
            } else {
                format!("; searched: {}", self.bwrap.searched_str(8))
            }
        )
    }

    /// Guidance for a `font_dir_unreadable`/`sandbox_unverified` font failure.
    pub fn font_note(&self) -> String {
        let mut s = format!(
            "fonts {} [{}]",
            self.font_dir
                .path
                .as_ref()
                .map_or_else(|| "<unset>".into(), |p| p.display().to_string()),
            self.font_dir.source_str()
        );
        if self.discovery_disabled && self.font_dir.path.is_none() {
            s.push_str(&format!(
                "; discovery is disabled by {DISCOVERY_ENV}, so only HAE_FONT_DIR is used"
            ));
        }
        if !self.font_dir.searched.is_empty() {
            s.push_str(&format!("; searched: {}", self.font_dir.searched_str(12)));
        }
        if let Some(n) = &self.font_dir.note {
            s.push_str(&format!("; {n}"));
        }
        s
    }
}

/// Probe candidates in order; the first viable one wins. Every examined
/// candidate is recorded, including the accepted one.
pub fn probe(candidates: &[Candidate], expect: Expect) -> ProbeOutcome {
    let mut searched = Vec::new();
    for c in candidates {
        let (verdict, detail) = classify(&c.path, expect);
        searched.push(Examined {
            path: c.path.clone(),
            origin: c.origin,
            verdict,
            detail,
        });
        if verdict == Verdict::Found {
            return ProbeOutcome {
                path: Some(c.path.clone()),
                source: Some(c.origin),
                searched,
                note: None,
            };
        }
    }
    ProbeOutcome {
        path: None,
        source: None,
        searched,
        note: None,
    }
}

fn classify(path: &Path, expect: Expect) -> (Verdict, Option<String>) {
    if expect == Expect::FontDirectory {
        return match std::fs::metadata(path) {
            Err(_) => (Verdict::Missing, None),
            Ok(md) if !md.is_dir() => (Verdict::NotDirectory, None),
            Ok(_) => {
                let (v, d) = font_verdict(&crate::fonts::inspect_font_dir(path));
                (v, Some(d))
            }
        };
    }
    let verdict = match std::fs::metadata(path) {
        Err(_) => Verdict::Missing,
        Ok(md) => match expect {
            Expect::Executable => {
                if !md.is_file() {
                    Verdict::NotExecutable
                } else if is_executable(&md) {
                    Verdict::Found
                } else {
                    Verdict::NotExecutable
                }
            }
            Expect::Directory | Expect::FontDirectory => {
                if md.is_dir() {
                    Verdict::Found
                } else {
                    Verdict::NotDirectory
                }
            }
        },
    };
    (verdict, None)
}

fn is_executable(md: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        md.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = md;
        // Windows has no execute bit; existence as a regular file is the
        // strongest check available without opening the image.
        true
    }
}

/// Small helper: a path-like value from an environment variable.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::TempDir;
    use std::fs;

    fn write_exec(path: &Path) {
        fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn paths(v: &[Candidate]) -> Vec<String> {
        v.iter().map(|c| c.path.display().to_string()).collect()
    }

    fn origins(v: &[Candidate]) -> Vec<&'static str> {
        v.iter().map(|c| c.origin.as_str()).collect()
    }

    fn temp() -> TempDir {
        TempDir::new_in(&std::env::temp_dir(), "hae-discovery-").unwrap()
    }

    #[test]
    fn env_is_used_as_given_and_never_replaced() {
        // A broken explicit value stays broken: it must not fall back to a
        // working sibling (the adapter reports `renderer_missing` instead).
        let dir = temp();
        let sibling = dir.path().join(tool_name("rhwp"));
        write_exec(&sibling);
        let ov = Overrides {
            rhwp: Some(PathBuf::from("/nonexistent/rhwp")),
            discovery: true,
            ..Default::default()
        };
        let sp = SearchPaths {
            exe_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let r = ResolvedTools::resolve(&ov, &sp);
        assert_eq!(r.rhwp.path, Some(PathBuf::from("/nonexistent/rhwp")));
        assert_eq!(r.rhwp.source_str(), "env");
        assert!(r.rhwp_failure().is_none(), "explicit value is configured");
    }

    #[test]
    fn candidate_priority_is_sibling_then_parent_bin_then_dev_tree_then_path() {
        let dir = temp();
        let root = dir.path().to_path_buf();
        let exe = root.join("target").join("debug");
        fs::create_dir_all(&exe).unwrap();
        let sp = SearchPaths {
            exe_dir: Some(exe.clone()),
            path_dirs: vec![PathBuf::from("/usr/bin")],
            roots: vec![root.clone()],
            ..Default::default()
        };
        let name = tool_name("rhwp");
        let cands = rhwp_candidates(&sp);
        assert_eq!(
            paths(&cands)[..5],
            [
                exe.join(&name).display().to_string(),
                root.join("target")
                    .join("bin")
                    .join(&name)
                    .display()
                    .to_string(),
                root.join(DEV_TREE)
                    .join("release")
                    .join(&name)
                    .display()
                    .to_string(),
                root.join(DEV_TREE)
                    .join("debug")
                    .join(&name)
                    .display()
                    .to_string(),
                dir.path()
                    .parent()
                    .unwrap()
                    .join(DEV_TREE)
                    .join("release")
                    .join(&name)
                    .display()
                    .to_string(),
            ]
        );
        assert_eq!(
            origins(&cands)[..4],
            ["exeDir", "exeParentBin", "devTreeRelease", "devTreeDebug"]
        );
        assert_eq!(*origins(&cands).last().unwrap(), "path");
        assert_eq!(paths(&cands).last().unwrap(), "/usr/bin/rhwp");
    }

    #[test]
    fn probe_accepts_the_first_viable_candidate_and_records_the_rest() {
        let dir = temp();
        let missing = dir.path().join("nope");
        let not_exec = dir.path().join("rhwp");
        fs::write(&not_exec, b"not executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&not_exec, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let good = dir.path().join("good-rhwp");
        write_exec(&good);
        let cands = vec![
            Candidate::new(&missing, Origin::ExeDir),
            Candidate::new(&not_exec, Origin::ExeParentBin),
            Candidate::new(&good, Origin::Path),
        ];
        let out = probe(&cands, Expect::Executable);
        assert_eq!(out.path.as_deref(), Some(good.as_path()));
        assert_eq!(out.source_str(), "path");
        #[cfg(unix)]
        assert_eq!(
            out.searched.iter().map(|e| e.verdict).collect::<Vec<_>>(),
            [Verdict::Missing, Verdict::NotExecutable, Verdict::Found]
        );
        #[cfg(not(unix))]
        assert_eq!(
            out.searched.iter().map(|e| e.verdict).collect::<Vec<_>>(),
            [Verdict::Missing, Verdict::Found, Verdict::Found]
        );
        assert!(out.searched_str(8).contains("missing"));
    }

    #[test]
    fn a_directory_is_never_accepted_as_an_executable() {
        let dir = temp();
        let d = dir.path().join("rhwp");
        fs::create_dir(&d).unwrap();
        let out = probe(&[Candidate::new(&d, Origin::ExeDir)], Expect::Executable);
        assert!(!out.is_found());
        assert_eq!(out.searched[0].verdict, Verdict::NotExecutable);
    }

    #[test]
    fn a_file_is_never_accepted_as_a_font_directory() {
        let dir = temp();
        let f = dir.path().join("fonts");
        fs::write(&f, b"x").unwrap();
        let out = probe(
            &[Candidate::new(&f, Origin::SystemFontDir)],
            Expect::Directory,
        );
        assert!(!out.is_found());
        assert_eq!(out.searched[0].verdict, Verdict::NotDirectory);
    }

    #[test]
    fn failed_search_names_the_fix_and_every_examined_candidate() {
        let dir = temp();
        let sp = SearchPaths {
            exe_dir: Some(dir.path().join("app")),
            path_dirs: vec![dir.path().join("p1")],
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let ov = Overrides {
            discovery: true,
            ..Default::default()
        };
        let r = ResolvedTools::resolve(&ov, &sp);
        let f = r.rhwp_failure().expect("no renderer found");
        assert!(f.message.contains("set HAE_RHWP"), "{}", f.message);
        assert!(
            f.message.contains("beside this application"),
            "{}",
            f.message
        );
        assert!(f.message.contains("searched: "), "{}", f.message);
        assert!(
            f.message
                .contains(&dir.path().join("app").display().to_string()),
            "{}",
            f.message
        );
        assert!(!f.searched.is_empty());
        let e = f.to_error();
        assert_eq!(e.reason(), Some("renderer_not_configured"));
        assert_eq!(e.diagnostic().detail.as_ref().unwrap()["tool"], "rhwp");
        assert!(!e.diagnostic().detail.as_ref().unwrap()["searched"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn disabling_discovery_keeps_only_the_environment_values() {
        let dir = temp();
        let sibling = dir.path().join(tool_name("rhwp"));
        write_exec(&sibling);
        let sp = SearchPaths {
            exe_dir: Some(dir.path().to_path_buf()),
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let ov = Overrides::from_values(|k| (k == DISCOVERY_ENV).then(|| OsString::from("0")));
        assert!(!ov.discovery);
        let r = ResolvedTools::resolve(&ov, &sp);
        assert!(!r.rhwp.is_found());
        assert!(r.rhwp.searched.is_empty(), "no candidate was examined");
        let f = r.rhwp_failure().unwrap();
        assert!(f.message.contains(DISCOVERY_ENV), "{}", f.message);
    }

    #[test]
    fn discovery_switch_parses_only_the_documented_off_values() {
        for off in ["0", "false", "FALSE", " off ", "No"] {
            assert!(
                !discovery_enabled(Some(OsStr::new(off))),
                "{off} should disable discovery"
            );
        }
        for on in ["", "1", "true", "yes", "on", "maybe"] {
            assert!(
                discovery_enabled(Some(OsStr::new(on))),
                "{on} should leave discovery on"
            );
        }
        assert!(discovery_enabled(None), "unset leaves discovery on");
    }

    #[test]
    fn sibling_discovery_wins_over_the_dev_tree_and_path() {
        let dir = temp();
        let exe = dir.path().join("release");
        fs::create_dir_all(&exe).unwrap();
        let sibling = exe.join(tool_name("rhwp"));
        write_exec(&sibling);
        let dev = dir.path().join(DEV_TREE).join("release");
        fs::create_dir_all(&dev).unwrap();
        write_exec(&dev.join(tool_name("rhwp")));
        let path_dir = dir.path().join("path");
        fs::create_dir_all(&path_dir).unwrap();
        write_exec(&path_dir.join(tool_name("rhwp")));
        let sp = SearchPaths {
            exe_dir: Some(exe.clone()),
            path_dirs: vec![path_dir],
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let r = ResolvedTools::resolve(
            &Overrides {
                discovery: true,
                ..Default::default()
            },
            &sp,
        );
        assert_eq!(r.rhwp.path.as_deref(), Some(sibling.as_path()));
        assert_eq!(r.rhwp.source_str(), "exeDir");
    }

    #[test]
    fn bwrap_keeps_the_documented_default_before_path() {
        let sp = SearchPaths {
            path_dirs: vec![PathBuf::from("/opt/bin")],
            ..Default::default()
        };
        let got = paths(&bwrap_candidates(&sp));
        assert_eq!(
            got,
            [
                "/usr/bin/bwrap",
                "/bin/bwrap",
                "/usr/local/bin/bwrap",
                "/opt/bin/bwrap"
            ]
        );
    }

    #[test]
    fn bwrap_is_found_on_path_when_the_defaults_are_absent() {
        let dir = temp();
        let bwrap = dir.path().join("bwrap");
        write_exec(&bwrap);
        let sp = SearchPaths {
            path_dirs: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let ov = Overrides {
            discovery: true,
            ..Default::default()
        };
        let r = ResolvedTools::resolve(&ov, &sp);
        // /usr/bin/bwrap exists on every supported Linux system, so the real
        // resolution is asserted only when the defaults are genuinely absent.
        if !Path::new("/usr/bin/bwrap").exists() && !Path::new("/bin/bwrap").exists() {
            assert_eq!(r.bwrap.path.as_deref(), Some(bwrap.as_path()));
            assert_eq!(r.bwrap.source_str(), "path");
        } else {
            assert_eq!(r.bwrap.source_str(), "default");
        }
    }

    #[test]
    fn font_candidates_prefer_app_and_user_dirs_before_the_system_dirs() {
        let home = PathBuf::from("/home/someone");
        let sp = SearchPaths {
            exe_dir: Some(PathBuf::from("/opt/hae/bin")),
            home: Some(home.clone()),
            ..Default::default()
        };
        let cands = font_dir_candidates(&sp);
        let got = paths(&cands);
        assert_eq!(got[0], "/opt/hae/bin/fonts");
        assert_eq!(
            got[1],
            home.join(".local/share/fonts").display().to_string()
        );
        assert_eq!(got[2], home.join(".fonts").display().to_string());
        assert_eq!(
            origins(&cands)[..3],
            ["appFontDir", "userFontDir", "userFontDir"]
        );
        // `/usr/share/fonts` keeps its place as the first system directory.
        let first_system = got.iter().position(|p| p == "/usr/share/fonts").unwrap();
        assert_eq!(first_system, 3);
    }

    /// A directory over the per-file font limit, cheaply (sparse file).
    fn oversized_fonts(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        let f = fs::File::create(dir.join("Huge.ttf")).unwrap();
        f.set_len(crate::fonts::MAX_FONT_FILE_BYTES + 1).unwrap();
    }

    fn small_fonts(dir: &Path, names: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        for n in names {
            fs::write(dir.join(n), b"not a real font, scan only counts bytes").unwrap();
        }
    }

    #[test]
    fn an_oversized_first_candidate_is_skipped_for_the_next_usable_one() {
        // Regression: the first existing directory used to win even when the
        // font limits rejected it, so every preview failed later with a
        // generic memory error although a usable directory was available.
        let t = temp();
        let home = t.path().join("home");
        oversized_fonts(&home.join(".local/share/fonts"));
        small_fonts(&home.join(".fonts"), &["NanumGothic.ttf"]);
        let sp = SearchPaths {
            home: Some(home.clone()),
            ..Default::default()
        };
        let r = ResolvedTools::resolve(
            &Overrides {
                discovery: true,
                ..Default::default()
            },
            &sp,
        );
        assert_eq!(
            r.font_dir.path,
            Some(home.join(".fonts")),
            "{:?}",
            r.font_dir
        );
        assert_eq!(r.font_dir.source_str(), "userFontDir");
        let skipped = &r.font_dir.searched[0];
        assert_eq!(skipped.verdict, Verdict::Unusable);
        assert!(
            skipped.detail.as_deref().unwrap().contains("per-file"),
            "{skipped}"
        );
        assert!(r.font_dir.note.is_none(), "a whole directory needs no note");
    }

    #[test]
    fn an_empty_font_directory_is_not_selected() {
        let t = temp();
        let empty = t.path().join("empty");
        let good = t.path().join("good");
        fs::create_dir_all(&empty).unwrap();
        small_fonts(&good, &["a.ttf"]);
        let out = select_font_dir(&[
            Candidate::new(&empty, Origin::AppFontDir),
            Candidate::new(&good, Origin::UserFontDir),
        ]);
        assert_eq!(out.path, Some(good));
        assert_eq!(out.searched[0].detail.as_deref(), Some("no font files"));
    }

    #[test]
    fn an_explicit_font_dir_is_never_probed_or_replaced() {
        let t = temp();
        let home = t.path().join("home");
        small_fonts(&home.join(".fonts"), &["a.ttf"]);
        let bad = t.path().join("oversized");
        oversized_fonts(&bad);
        let sp = SearchPaths {
            home: Some(home),
            ..Default::default()
        };
        let r = ResolvedTools::resolve(
            &Overrides {
                font_dir: Some(bad.clone()),
                discovery: true,
                ..Default::default()
            },
            &sp,
        );
        assert_eq!(r.font_dir.path, Some(bad));
        assert_eq!(r.font_dir.source_str(), "env");
        assert!(r.font_dir.searched.len() == 1 && r.font_dir.note.is_none());
    }

    #[test]
    fn oversized_system_dir_falls_back_to_a_reported_bounded_subset() {
        let t = temp();
        let root = t.path().join("fonts");
        oversized_fonts(&root.join("big"));
        small_fonts(&root.join("a-latin"), &["Latin.ttf"]);
        small_fonts(
            &root.join("b-korean"),
            &["NanumGothic.ttf", "NanumGothicBold.ttf"],
        );
        small_fonts(&root.join("c-latin"), &["Other.otf"]);
        fs::create_dir_all(root.join("d-empty")).unwrap();
        // The root itself exceeds the limit, so no whole directory is usable.
        oversized_fonts(&root);
        let out = select_font_dir(&[Candidate::new(&root, Origin::SystemFontDir)]);
        assert_eq!(out.path, Some(root.join("b-korean")), "{out:?}");
        assert_eq!(out.source_str(), "systemFontSubdir");
        let note = out.note.as_deref().expect("the narrowing is reported");
        assert!(note.contains("b-korean"), "{note}");
        assert!(note.contains("bounded subset"), "{note}");
        for omitted in ["a-latin", "c-latin", "big", "d-empty"] {
            assert!(note.contains(omitted), "{omitted} not named in: {note}");
        }
        let verdicts: Vec<_> = out
            .searched
            .iter()
            .map(|e| {
                (
                    e.path.file_name().unwrap().to_str().unwrap().to_string(),
                    e.verdict,
                )
            })
            .collect();
        assert!(
            verdicts.contains(&("big".into(), Verdict::Unusable)),
            "{verdicts:?}"
        );
        // Deterministic: the same inputs give the same choice.
        let again = select_font_dir(&[Candidate::new(&root, Origin::SystemFontDir)]);
        assert_eq!(again.path, out.path);
        assert_eq!(again.note, out.note);
        // Without a Korean-named font the first usable subdirectory wins.
        fs::remove_dir_all(root.join("b-korean")).unwrap();
        let out = select_font_dir(&[Candidate::new(&root, Origin::SystemFontDir)]);
        assert_eq!(out.path, Some(root.join("a-latin")));
    }

    #[test]
    fn nothing_usable_stays_unresolved_with_every_reason_recorded() {
        let t = temp();
        let bad = t.path().join("bad");
        oversized_fonts(&bad);
        let out = select_font_dir(&[
            Candidate::new(t.path().join("missing"), Origin::AppFontDir),
            Candidate::new(&bad, Origin::UserFontDir),
        ]);
        assert!(!out.is_found());
        assert_eq!(out.searched.len(), 2);
        assert_eq!(out.searched[0].verdict, Verdict::Missing);
        assert_eq!(out.searched[1].verdict, Verdict::Unusable);
    }

    #[test]
    fn raster_candidates_are_siblings_only() {
        let dir = temp();
        let exe = dir.path().join("target").join("release");
        let sp = SearchPaths {
            exe_dir: Some(exe.clone()),
            path_dirs: vec![PathBuf::from("/usr/bin")],
            ..Default::default()
        };
        let cands = raster_candidates(&sp);
        assert_eq!(
            paths(&cands),
            [
                exe.join(tool_name("hae-raster")).display().to_string(),
                dir.path()
                    .join("target")
                    .join("bin")
                    .join(tool_name("hae-raster"))
                    .display()
                    .to_string(),
            ]
        );
        assert!(paths(&cands).iter().all(|p| !p.starts_with("/usr/bin")));
    }

    #[test]
    fn deps_directory_is_stripped_for_the_sibling_rule() {
        let dir = temp();
        let deps = dir.path().join("target").join("debug").join("deps");
        fs::create_dir_all(&deps).unwrap();
        let sp = SearchPaths {
            exe_dir: Some(strip_deps_dir(&deps)),
            ..Default::default()
        };
        assert_eq!(
            sp.exe_dir.as_deref(),
            Some(dir.path().join("target/debug").as_path())
        );
        assert_eq!(
            rhwp_candidates(&sp)[0].path,
            dir.path().join("target/debug").join(tool_name("rhwp"))
        );
    }

    #[test]
    fn explicit_values_are_recorded_as_env_and_never_probed() {
        let r = ResolvedTools::all_explicit(
            Path::new("/x/rhwp"),
            Path::new("/usr/bin/bwrap"),
            Path::new("/x/fonts"),
            Path::new("/x/hae-raster"),
        );
        assert_eq!(r.rhwp.source_str(), "explicit");
        assert!(r.rhwp.searched.is_empty());
        assert_eq!(r.bwrap.source_str(), "explicit");
        assert!(r.rhwp_failure().is_none());
    }
}
