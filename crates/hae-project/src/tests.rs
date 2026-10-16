use super::*;
use hae_diag::{Code, Limits};
use hae_model::Document;
use std::{fs, path::PathBuf};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(hae_model::new_id("hae-project-").unwrap());
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn document() -> Document {
    Document::open(
        include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
#[test]
fn save_reopens_stable_ids_and_holds_lock_until_binding_dropped() {
    let t = Temp::new();
    let roots = IoRoots::new(&t.0, &t.0, Limits::default()).unwrap();
    let doc = document();
    let p = t.0.join("project.hwpwork");
    let mut binding = None;
    roots
        .save_project(&mut binding, &p, &doc, 7, |_| Ok(()))
        .unwrap();
    assert_eq!(
        roots.open_project(&p).unwrap_err().code(),
        Code::ProjectLocked
    );
    drop(binding.take());
    let reopened = roots.open_project(&p).unwrap();
    assert_eq!(reopened.revision, 7);
    assert_eq!(doc.map_rows(), reopened.document.map_rows());
    assert_eq!(
        doc.original.raw_bytes(),
        reopened.document.original.raw_bytes()
    );
}
#[test]
fn validation_failure_does_not_publish_or_bind() {
    let t = Temp::new();
    let roots = IoRoots::new(&t.0, &t.0, Limits::default()).unwrap();
    let p = t.0.join("project.hwpwork");
    let mut binding = None;
    let e = roots
        .save_project(&mut binding, &p, &document(), 0, |_| {
            Err(hae_diag::EngineError::new(
                Code::VerifyFailed,
                hae_diag::Stage::Save,
                "injected mismatch",
            ))
        })
        .unwrap_err();
    assert_eq!(e.code(), Code::VerifyFailed);
    assert!(!p.exists());
    assert!(binding.is_none());
}
#[test]
fn external_modification_and_lock_replacement_are_rejected() {
    let t = Temp::new();
    let roots = IoRoots::new(&t.0, &t.0, Limits::default()).unwrap();
    let p = t.0.join("project.hwpwork");
    let mut binding = None;
    let doc = document();
    roots
        .save_project(&mut binding, &p, &doc, 0, |_| Ok(()))
        .unwrap();
    fs::write(&p, b"external").unwrap();
    assert_eq!(
        roots
            .save_project(&mut binding, &p, &doc, 0, |_| Ok(()))
            .unwrap_err()
            .code(),
        Code::ExternalModification
    );
    let lock = t.0.join("project.hwpwork.lock");
    fs::remove_file(&lock).unwrap();
    fs::write(&lock, b"").unwrap();
    assert_eq!(
        roots
            .save_project(&mut binding, &p, &doc, 0, |_| Ok(()))
            .unwrap_err()
            .code(),
        Code::ProjectLocked
    );
}
#[test]
fn export_cannot_overwrite_source_or_existing_or_follow_links() {
    let t = Temp::new();
    let roots = IoRoots::new(&t.0, &t.0, Limits::default()).unwrap();
    let src = t.0.join("source.hwpx");
    let bytes = document().to_hwpx().unwrap();
    fs::write(&src, &bytes).unwrap();
    roots.read_input(&src).unwrap();
    assert_eq!(
        roots
            .export_hwpx(&src, &document(), |_| Ok(()))
            .unwrap_err()
            .code(),
        Code::PathNotAllowed
    );
    let dest = t.0.join("export.hwpx");
    roots.export_hwpx(&dest, &document(), |_| Ok(())).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), bytes);
    assert_eq!(
        roots
            .export_hwpx(&dest, &document(), |_| Ok(()))
            .unwrap_err()
            .code(),
        Code::TargetExists
    );
    std::os::unix::fs::symlink(&src, t.0.join("link.hwpx")).unwrap();
    assert!(roots.read_input(t.0.join("link.hwpx")).is_err());
    fs::hard_link(&src, t.0.join("hard.hwpx")).unwrap();
    assert!(roots.read_input(t.0.join("hard.hwpx")).is_err());
}

#[test]
fn fault_injection_preserves_state_before_rename_and_updates_binding_on_parent_fsync_error() {
    let t = Temp::new();
    let roots = IoRoots::new(&t.0, &t.0, Limits::default()).unwrap();
    let p = t.0.join("project.hwpwork");
    let doc = document();
    let mut binding = None;

    for fault in [
        FaultStep::TempWrite,
        FaultStep::TempFsync,
        FaultStep::Rename,
    ] {
        roots.set_fault(Some(fault));
        assert_eq!(
            roots
                .save_project(&mut binding, &p, &doc, 1, |_| Ok(()))
                .unwrap_err()
                .code(),
            Code::IoError
        );
        assert!(!p.exists());
        assert!(binding.is_none());
    }

    // ParentFsync failure occurs after rename: file is published and binding is updated.
    roots.set_fault(Some(FaultStep::ParentFsync));
    assert_eq!(
        roots
            .save_project(&mut binding, &p, &doc, 2, |_| Ok(()))
            .unwrap_err()
            .code(),
        Code::SaveDurabilityUncertain
    );
    assert!(p.exists());
    assert!(binding.is_some());

    // Subsequent save succeeds because binding has the published file's hash.
    roots
        .save_project(&mut binding, &p, &doc, 3, |_| Ok(()))
        .unwrap();
    drop(binding.take());
    let reopened = roots.open_project(&p).unwrap();
    assert_eq!(reopened.revision, 3);
}

#[test]
fn tampered_project_manifest_or_idmap_returns_project_integrity_error() {
    let t = Temp::new();
    let roots = IoRoots::new(&t.0, &t.0, Limits::default()).unwrap();
    let p = t.0.join("project.hwpwork");
    let doc = document();
    let mut binding = None;
    roots
        .save_project(&mut binding, &p, &doc, 4, |_| Ok(()))
        .unwrap();
    drop(binding.take());

    let orig_bytes = fs::read(&p).unwrap();
    let archive = hae_zip::Archive::open(orig_bytes, &Limits::default()).unwrap();

    // Tamper manifest revision without updating current_file_sha256 / manifest hash
    let mut bad_manifest = hae_zip::keep_all(&archive);
    for item in &mut bad_manifest {
        if let hae_zip::OutEntry::Keep(name) = item {
            if name == "manifest.json" {
                let s = std::str::from_utf8(archive.payload("manifest.json").unwrap())
                    .unwrap()
                    .replace("\"revision\":4", "\"revision\":999");
                let mut m: serde_json::Value = serde_json::from_str(&s).unwrap();
                m["currentFileSha256"] = serde_json::Value::String("0".repeat(64));
                *item = hae_zip::OutEntry::Replace(
                    name.clone(),
                    std::sync::Arc::from(serde_json::to_vec(&m).unwrap()),
                );
            }
        }
    }
    let p_bad = t.0.join("bad_manifest.hwpwork");
    fs::write(&p_bad, hae_zip::write(&archive, &bad_manifest).unwrap()).unwrap();
    assert_eq!(
        roots.open_project(&p_bad).unwrap_err().code(),
        Code::ProjectIntegrityError
    );
}
