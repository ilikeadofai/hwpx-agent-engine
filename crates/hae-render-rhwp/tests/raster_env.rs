//! Worker location policy: `HAE_RASTER` and the sibling default. Kept in its
//! own test binary because it edits process environment variables.

mod common;
use common::raster_bin;
use hae_render_rhwp::worker::default_worker_path;
use hae_render_rhwp::AdapterConfig;
use std::path::PathBuf;

#[test]
fn worker_path_comes_from_hae_raster_else_the_sibling_default() {
    // Test executables live in target/<profile>/deps; the worker is one up.
    assert_eq!(
        default_worker_path().canonicalize().unwrap(),
        raster_bin().canonicalize().unwrap()
    );
    assert_eq!(
        AdapterConfig::new("/x/rhwp", "/x/fonts").raster_path,
        default_worker_path()
    );

    std::env::set_var("HAE_RHWP", "/x/rhwp");
    std::env::set_var("HAE_RASTER", "/explicit/hae-raster");
    let c = AdapterConfig::from_env().unwrap();
    assert_eq!(c.raster_path, PathBuf::from("/explicit/hae-raster"));
    std::env::set_var("HAE_RASTER", "");
    assert_eq!(
        AdapterConfig::from_env().unwrap().raster_path,
        default_worker_path()
    );
    std::env::remove_var("HAE_RASTER");
    assert_eq!(
        AdapterConfig::from_env().unwrap().raster_path,
        default_worker_path()
    );
    std::env::remove_var("HAE_RHWP");
}
