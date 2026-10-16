//! Shared helpers for the adapter integration tests.
#![allow(dead_code)]

use hae_render_rhwp::sandbox::TempDir;
use hae_render_rhwp::{AdapterConfig, RhwpAdapter};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn corpus(name: &str) -> PathBuf {
    workspace_root().join("tests/fixtures/corpus").join(name)
}

/// The rhwp binary under test: `HAE_RHWP`, else the spike build. Tests that
/// need it fail loudly if neither exists.
pub fn rhwp_path() -> PathBuf {
    if let Some(p) = std::env::var_os("HAE_RHWP").filter(|v| !v.is_empty()) {
        return p.into();
    }
    workspace_root().join("spikes/vendor/rhwp-src/target/release/rhwp")
}

pub struct Env {
    pub _fonts: TempDir,
    pub adapter: RhwpAdapter,
}

/// Font directory with one Korean font when the system has it, else empty.
pub fn font_dir() -> TempDir {
    let td = TempDir::new_in(&std::env::temp_dir(), "hae-test-fonts-").unwrap();
    for cand in [
        "/usr/local/share/fonts/n/NanumGothic.ttf",
        "/usr/share/fonts/nanum/NanumGothic.ttf",
        "/usr/share/fonts/TTF/NanumGothic.ttf",
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
    ] {
        if Path::new(cand).is_file() {
            fs::copy(cand, td.path().join("NanumGothic.ttf")).unwrap();
            break;
        }
    }
    td
}

pub fn real_env() -> Env {
    let fonts = font_dir();
    let cfg = AdapterConfig::new(rhwp_path(), fonts.path());
    Env {
        adapter: RhwpAdapter::new(cfg),
        _fonts: fonts,
    }
}

/// Adapter whose "rhwp" is a shell script (runs inside the sandbox via /usr/bin/sh).
pub fn fake_env(script_body: &str, tweak: impl FnOnce(&mut AdapterConfig)) -> (Env, TempDir) {
    let dir = TempDir::new_in(&std::env::temp_dir(), "hae-test-fake-").unwrap();
    let bin = dir.path().join("fake_rhwp");
    fs::write(
        &bin,
        format!(
            "#!/usr/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'rhwp vfake'; exit 0; fi\n{script_body}\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    let fonts = font_dir();
    let mut cfg = AdapterConfig::new(&bin, fonts.path());
    tweak(&mut cfg);
    (
        Env {
            adapter: RhwpAdapter::new(cfg),
            _fonts: fonts,
        },
        dir,
    )
}
