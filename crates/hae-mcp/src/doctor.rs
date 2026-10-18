//! Local dependency/permission and bounded font preflight checks. No settings are written and no renderer is executed.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

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
    json!({"ok":ok,"path":path,"message":if ok {"executable found"} else {"missing executable or configuration"}})
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

fn font_check(path: &Path) -> Value {
    use hae_render_rhwp::fonts::{
        inspect_font_dir, MAX_FONT_BYTES, MAX_FONT_FILES, MAX_FONT_FILE_BYTES,
    };
    let scan = inspect_font_dir(path);
    let warning = (scan.korean_filename_matches == 0).then_some(
        "한국어 글꼴로 알려진 파일명이 없습니다. 파일명 휴리스틱이므로 실제 한글 지원 여부는 별도 확인하세요.");
    json!({
        "ok": scan.error.is_none(), "path": path,
        "message": scan.error.as_deref().unwrap_or("bounded font preflight passed"),
        "fileCount": scan.file_count, "totalBytes": scan.total_bytes,
        "measurementsComplete": scan.measurements_complete,
        "limits": {"fileCount":MAX_FONT_FILES, "totalBytes":MAX_FONT_BYTES, "perFileBytes":MAX_FONT_FILE_BYTES},
        "koreanDetection":"filename heuristic (no cmap parsing)",
        "koreanFilenameMatches": scan.korean_filename_matches, "warning":warning,
        "guidance":"한국어 글꼴 몇 개만 담은 작은 폴더를 만들어 HAE_FONT_DIR로 지정하세요. 불완전한 측정값은 하한이며, 글꼴 유효성·실제 렌더링과 세션의 남은 메모리는 별도 확인이 필요합니다."
    })
}

pub fn report(input_root: &Path, output_root: &Path) -> Value {
    let env_path = |key: &str| {
        std::env::var_os(key)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    };
    let raster = env_path("HAE_RASTER").or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("hae-raster")))
    });
    let checks = json!({
        "rhwp":file_check(env_path("HAE_RHWP")),
        "raster":file_check(raster),
        "bwrap":file_check(Some(env_path("HAE_BWRAP").unwrap_or_else(|| PathBuf::from("/usr/bin/bwrap")))),
        "fonts":font_check(&env_path("HAE_FONT_DIR").unwrap_or_else(|| PathBuf::from("/usr/share/fonts"))),
        "inputRoot":root_check(input_root, false),
        "outputRoot":root_check(output_root, true),
    });
    let ok = checks
        .as_object()
        .unwrap()
        .values()
        .all(|c| c["ok"] == true);
    json!({"ok":ok,"version":env!("CARGO_PKG_VERSION"),"checks":checks,
        "scope":"Local paths, permissions and bounded font metadata preflight only; sandbox execution and native Hancom compatibility need separate end-to-end checks."})
}
