//! Local dependency/permission checks. No settings are written and no renderer is executed.
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
        "fonts":root_check(&env_path("HAE_FONT_DIR").unwrap_or_else(|| PathBuf::from("/usr/share/fonts")), false),
        "inputRoot":root_check(input_root, false),
        "outputRoot":root_check(output_root, true),
    });
    let ok = checks
        .as_object()
        .unwrap()
        .values()
        .all(|c| c["ok"] == true);
    json!({"ok":ok,"version":env!("CARGO_PKG_VERSION"),"checks":checks,
        "scope":"Local paths and permissions only; sandbox execution and native Hancom compatibility need separate end-to-end checks."})
}
