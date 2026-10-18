//! Local dependency/permission and bounded font preflight checks. No settings are written and no renderer is executed.
//!
//! The preview tools are reported with the same discovery rules the adapter
//! uses ([`hae_render_rhwp::preview_environment`]): the path that was accepted,
//! which rule accepted it, which candidates were examined first, and one line
//! telling the user what to do when nothing was found.
use hae_render_rhwp::discovery::{Expect, ProbeOutcome};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

/// What to do about a candidate list that produced nothing, per tool.
fn fix_for(tool: &str) -> &'static str {
    match tool {
        "rhwp" => {
            "`HAE_RHWP=/path/to/rhwp`를 지정하거나 실행 파일 옆(또는 PATH, 저장소의 \
             spikes/vendor/rhwp-src/target/{release,debug})에 두세요."
        }
        "bwrap" => "`HAE_BWRAP=/path/to/bwrap`을 지정하거나 /usr/bin/bwrap에 설치하세요.",
        "raster" => {
            "`HAE_RASTER=/path/to/hae-raster`를 지정하거나 실행 파일과 같은 폴더에 두세요. \
             PNG 미리보기에는 이 작업 파일이 필요합니다."
        }
        "fonts" => "`HAE_FONT_DIR`를 읽을 수 있고 글꼴 한도(파일 수·용량) 안에 드는 글꼴 폴더로 지정하세요.",
        _ => "경로를 명시적으로 지정하세요.",
    }
}

/// One tool: provenance, the accepted path and the examined candidates.
fn tool_check(tool: &str, probe: &ProbeOutcome, expect: Expect, enabled: bool) -> Value {
    let ok = probe.path.as_deref().is_some_and(|p| accepts(p, expect));
    let message = if ok {
        format!(
            "found at {} ({})",
            probe
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            probe.source_str()
        )
    } else if enabled {
        format!("not found; {}", fix_for(tool))
    } else {
        format!(
            "not configured and discovery is disabled ({}); {}",
            hae_render_rhwp::discovery::DISCOVERY_ENV,
            fix_for(tool)
        )
    };
    json!({
        "ok": ok,
        "path": probe.path,
        "source": probe.source_str(),
        "message": message,
        "searched": probe.searched.iter().map(|e| json!({
            "path": e.path, "origin": e.origin, "verdict": e.verdict, "detail": e.detail,
        })).collect::<Vec<_>>(),
        "guidance": if ok { Value::Null } else { json!(fix_for(tool)) },
    })
}

/// Whether a configured path is usable for its purpose. Explicit values are
/// re-checked here so a typo stays `ok:false` in `doctor`.
fn accepts(path: &Path, expect: Expect) -> bool {
    match expect {
        Expect::Executable => file_check(Some(path.to_path_buf()))["ok"] == true,
        Expect::Directory | Expect::FontDirectory => path.is_dir(),
    }
}

fn file_check(path: Option<PathBuf>) -> Value {
    let ok = path.as_ref().is_some_and(|p| {
        if !p.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            true
        }
    });
    #[cfg(unix)]
    let success_message = "executable found";
    #[cfg(windows)]
    let success_message = "file found; Windows execution is not determined by Unix execute bits (launch not verified)";
    #[cfg(not(any(unix, windows)))]
    let success_message = "file found; executable status was not verified";
    json!({"ok":ok,"path":path,"message":if ok {success_message} else {"missing executable or configuration"}})
}

fn root_check(path: &Path, write: bool) -> Value {
    let check = || -> std::io::Result<PathBuf> {
        let canonical = path.canonicalize()?;
        if !canonical.is_dir() {
            return Err(std::io::Error::other("not a directory"));
        }
        fs::read_dir(&canonical)?;
        if write {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let probe = canonical.join(format!(".hae-doctor-{}-{stamp}", std::process::id()));
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)?;
            drop(file);
            fs::remove_file(probe)?;
        }
        Ok(canonical)
    };
    match check() {
        Ok(canonical) => {
            json!({"ok":true,"path":canonical,"message":if write {"readable and writable directory"} else {"readable directory"}})
        }
        Err(e) => json!({"ok":false,"path":path,"message":e.to_string()}),
    }
}

fn font_check(path: Option<&Path>, probe: &ProbeOutcome, enabled: bool) -> Value {
    use hae_render_rhwp::fonts::{
        inspect_font_dir, FontSet, MAX_FONT_BYTES, MAX_FONT_FILES, MAX_FONT_FILE_BYTES,
    };
    let located = path.is_some();
    let scan = path.map(inspect_font_dir);
    let error = scan.as_ref().and_then(|s| s.error.clone());
    let korean = scan.as_ref().map_or(0, |s| s.korean_filename_matches);
    let warning = (located && korean == 0).then_some(
        "한국어 글꼴로 알려진 파일명이 없습니다. 파일명 휴리스틱이므로 실제 한글 지원 여부는 별도 확인하세요.");
    let ok = located && error.is_none();
    // The exact set a preview would use: its hash is the one recorded in the
    // preview metadata. Only computed after the bounded preflight passed.
    let set = path
        .filter(|_| ok)
        .and_then(|p| FontSet::scan(p).ok())
        .map(|f| (f.hash.clone(), f.files.len()));
    json!({
        "ok": ok, "path": path,
        "source": probe.source_str(),
        "selection": probe.note,
        "searched": probe.searched.iter().map(|e| json!({
            "path": e.path, "origin": e.origin, "verdict": e.verdict, "detail": e.detail,
        })).collect::<Vec<_>>(),
        "message": if !located {
            if enabled { format!("no usable font directory found; {}", fix_for("fonts")) }
            else { format!("font directory not configured and discovery is disabled ({}); {}", hae_render_rhwp::discovery::DISCOVERY_ENV, fix_for("fonts")) }
        } else { error.clone().unwrap_or_else(|| "bounded font preflight passed".into()) },
        "fileCount": scan.as_ref().map(|s| s.file_count), "totalBytes": scan.as_ref().map(|s| s.total_bytes),
        "measurementsComplete": scan.as_ref().map(|s| s.measurements_complete),
        "fontSetSha256": set.as_ref().map(|s| s.0.clone()),
        "limits": {"fileCount":MAX_FONT_FILES, "totalBytes":MAX_FONT_BYTES, "perFileBytes":MAX_FONT_FILE_BYTES},
        "koreanDetection":"filename heuristic (no cmap parsing)",
        "koreanFilenameMatches": korean, "warning":warning,
        "guidance":"한국어 글꼴 몇 개만 담은 작은 폴더를 만들어 HAE_FONT_DIR로 지정하세요. 불완전한 측정값은 하한이며, 글꼴 유효성·실제 렌더링과 세션의 남은 메모리는 별도 확인이 필요합니다."
    })
}

/// Preview tools resolved exactly as the adapter resolves them, with the
/// platform isolation policy and what it can enforce.
fn preview_checks() -> Value {
    let env = hae_render_rhwp::preview_environment();
    let enabled = !env.tools.discovery_disabled;
    let fonts_path = env.tools.font_dir.path.clone();
    json!({
        "rhwp": tool_check("rhwp", &env.tools.rhwp, Expect::Executable, enabled),
        "raster": tool_check("raster", &env.tools.raster, Expect::Executable, enabled),
        "bwrap": tool_check("bwrap", &env.tools.bwrap, Expect::Executable, enabled),
        "fonts": font_check(fonts_path.as_deref(), &env.tools.font_dir, enabled),
        "discovery": {
            "enabled": enabled,
            "variable": hae_render_rhwp::discovery::DISCOVERY_ENV,
            "order": [
                "explicit HAE_* variable (always wins, never replaced)",
                "directory of the running executable",
                "parent directory bin/",
                "spikes/vendor/rhwp-src/target/{release,debug} below the working directory and the executable",
                "PATH (rhwp, bwrap only)",
                "fonts: <exe dir>/fonts, then per-user dirs, then whole system dirs; a dir over the font limits is skipped with its reason, and only if none is usable a bounded subdirectory of an oversized system dir is selected and reported under fonts.selection",
            ],
        },
        "platformIsolation": {
            "platform": std::env::consts::OS,
            "linuxContract": "bubblewrap namespaces + prlimit RLIMIT_AS, verified by a self-test before the first run",
            "unsandboxedOptIn": hae_render_rhwp::unsandboxed::ALLOW_ENV,
            "unsandboxedOptInSet": env.unsandboxed_policy.is_on(),
            "honoredOnThisPlatform": cfg!(windows),
            "enforcement": env.enforcement,
        },
    })
}

pub fn report(input_root: &Path, output_root: &Path) -> Value {
    let mut checks = preview_checks();
    checks["inputRoot"] = root_check(input_root, false);
    checks["outputRoot"] = root_check(output_root, true);
    let ok = checks
        .as_object()
        .unwrap()
        .values()
        .all(|c| c["ok"] == true || c["ok"].is_null());
    json!({"ok":ok,"version":env!("CARGO_PKG_VERSION"),"checks":checks,
        "scope":"Local paths, permissions and bounded font metadata preflight only; sandbox execution and native Hancom compatibility need separate end-to-end checks. Discovery results show which rule produced each path and which candidates were examined first."})
}
