use hae_diag::{Code, Limits, Stage};
use hae_project::{FaultStep, IoRoots};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
#[cfg(windows)]
use std::os::windows::fs::symlink_dir as symlink;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(hae_model::new_id("hae-preview-test-").unwrap());
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn roots(&self, limits: Limits) -> IoRoots {
        IoRoots::new(&self.0, &self.0, limits).unwrap()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

fn assert_file_metadata(path: &Path) {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.is_file());
    #[cfg(unix)]
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Windows has ACLs rather than POSIX mode bits. Assert the file was not
        // published as a reparse point. ACL privacy is inherited from the parent.
        assert_eq!(metadata.file_attributes() & 0x0000_0400, 0);
    }
}

#[test]
fn writes_exact_opaque_bytes_with_hash_revision_and_unique_direct_paths() {
    let input = Temp::new();
    let output = Temp::new();
    let roots = IoRoots::new(&input.0, &output.0, Limits::default()).unwrap();
    let mut paths = Vec::new();
    for revision in 0..16 {
        let extension = if revision % 2 == 0 { "png" } else { "svg" };
        // Image validation belongs to the caller: storage must not parse or
        // transform this deliberately opaque payload.
        let receipt = roots
            .write_preview_artifact(extension, b"abc", revision)
            .unwrap();
        assert!(receipt.path.is_absolute());
        assert_eq!(receipt.path.parent(), Some(output.0.as_path()));
        assert_eq!(receipt.path.extension().unwrap(), extension);
        assert_eq!(receipt.revision, revision);
        assert_eq!(receipt.sha256, ABC_SHA256);
        assert_eq!(fs::read(&receipt.path).unwrap(), b"abc");
        assert_file_metadata(&receipt.path);
        assert!(!paths.contains(&receipt.path));
        paths.push(receipt.path);
    }
    paths.sort();
    assert_eq!(entries(&output.0), paths); // No temporary files or subdirectories.
    assert!(entries(&input.0).is_empty());
}

#[test]
fn rejects_non_allowlisted_extensions_without_creating_any_output() {
    let temp = Temp::new();
    let roots = temp.roots(Limits::default());
    for extension in [
        "",
        "jpg",
        "PNG",
        "Svg",
        ".png",
        "../png",
        "png/escape",
        "svg\0",
        "png ",
    ] {
        let error = roots
            .write_preview_artifact(extension, b"abc", 1)
            .unwrap_err();
        assert_eq!(error.code(), Code::InvalidRequest, "{extension:?}");
        assert_eq!(error.diagnostic().stage, Stage::Render);
        assert!(entries(&temp.0).is_empty());
    }
}

#[test]
fn enforces_both_size_caps_before_writing_and_accepts_the_exact_boundary() {
    // Also covers floor division for odd memory limits and a zero-byte budget.
    for (project, memory, cap) in [(3, 100, 3), (100, 7, 3), (0, 100, 0), (100, 1, 0)] {
        let temp = Temp::new();
        let roots = temp.roots(Limits {
            max_project_bytes: project,
            max_session_memory: memory,
            ..Limits::default()
        });
        let error = roots
            .write_preview_artifact("png", &vec![42; cap + 1], 9)
            .unwrap_err();
        assert_eq!(error.code(), Code::ResourceLimit);
        assert_eq!(error.diagnostic().stage, Stage::Render);
        assert!(entries(&temp.0).is_empty());
        let receipt = roots
            .write_preview_artifact("svg", &vec![42; cap], 9)
            .unwrap();
        assert_eq!(fs::read(&receipt.path).unwrap(), vec![42; cap]);
        assert_eq!(entries(&temp.0), vec![receipt.path]);
    }
}

#[test]
fn refuses_symlink_swapped_output_root_without_writing_outside_or_to_the_old_root() {
    let temp = Temp::new();
    let output = temp.0.join("output");
    let moved = temp.0.join("moved");
    let outside = Temp::new();
    fs::create_dir(&output).unwrap();
    let roots = IoRoots::new(&temp.0, &output, Limits::default()).unwrap();
    fs::rename(&output, &moved).unwrap();
    if let Err(error) = symlink(&outside.0, &output) {
        if cfg!(windows)
            && matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
            )
        {
            // Windows symlink creation may require Developer Mode or a privilege.
            return;
        }
        panic!("could not create test symlink: {error}");
    }

    assert_eq!(
        roots
            .write_preview_artifact("png", b"abc", 3)
            .unwrap_err()
            .code(),
        Code::PathNotAllowed
    );
    assert!(entries(&outside.0).is_empty());
    assert!(entries(&moved).is_empty());
}

#[test]
fn refuses_replaced_output_root_directory_identity() {
    let temp = Temp::new();
    let output = temp.0.join("output");
    let moved = temp.0.join("moved");
    fs::create_dir(&output).unwrap();
    let roots = IoRoots::new(&temp.0, &output, Limits::default()).unwrap();
    fs::rename(&output, &moved).unwrap();
    fs::create_dir(&output).unwrap();

    assert_eq!(
        roots
            .write_preview_artifact("svg", b"abc", 3)
            .unwrap_err()
            .code(),
        Code::PathNotAllowed
    );
    assert!(entries(&output).is_empty());
    assert!(entries(&moved).is_empty());
}

#[test]
fn prepublication_faults_remove_temps_and_preserve_inputs_and_existing_artifacts() {
    for fault in [
        FaultStep::TempWrite,
        FaultStep::TempFsync,
        FaultStep::Rename,
    ] {
        let temp = Temp::new();
        let roots = temp.roots(Limits::default());
        let original = temp.0.join("input.png");
        fs::write(&original, b"protected source").unwrap();
        roots.read_input(&original).unwrap();
        let previous = roots
            .write_preview_artifact("svg", b"previous preview", 4)
            .unwrap();
        let before = entries(&temp.0);
        roots.set_fault(Some(fault));

        let error = roots.write_preview_artifact("png", b"abc", 5).unwrap_err();
        assert_eq!(error.code(), Code::IoError, "{fault:?}");
        assert_eq!(error.diagnostic().stage, Stage::Render);
        assert_eq!(entries(&temp.0), before, "{fault:?}: leaked output/temp");
        assert_eq!(fs::read(&original).unwrap(), b"protected source");
        assert_eq!(fs::read(&previous.path).unwrap(), b"previous preview");

        // The fault is consumed and a later request can still publish.
        roots.write_preview_artifact("png", b"abc", 5).unwrap();
    }
}

#[test]
fn parent_fsync_failure_reports_published_receipt_and_keeps_originals() {
    let temp = Temp::new();
    let roots = temp.roots(Limits::default());
    let original = temp.0.join("input.svg");
    fs::write(&original, b"protected source").unwrap();
    roots.read_input(&original).unwrap();
    let previous = roots
        .write_preview_artifact("png", b"previous preview", 7)
        .unwrap();
    roots.set_fault(Some(FaultStep::ParentFsync));

    let error = roots.write_preview_artifact("svg", b"abc", 42).unwrap_err();
    assert_eq!(error.code(), Code::SaveDurabilityUncertain);
    assert_eq!(error.diagnostic().stage, Stage::Render);
    let detail = error
        .diagnostic()
        .detail
        .as_ref()
        .expect("published receipt detail");
    let path = PathBuf::from(detail["path"].as_str().unwrap());
    assert!(path.is_absolute());
    assert_eq!(path.parent(), Some(temp.0.as_path()));
    assert_eq!(path.extension().unwrap(), "svg");
    assert_eq!(detail["sha256"], ABC_SHA256);
    assert_eq!(detail["revision"], 42);
    assert_eq!(fs::read(&path).unwrap(), b"abc");
    assert_file_metadata(&path);
    assert_eq!(fs::read(&original).unwrap(), b"protected source");
    assert_eq!(fs::read(&previous.path).unwrap(), b"previous preview");
    let mut expected = vec![original, previous.path, path];
    expected.sort();
    assert_eq!(entries(&temp.0), expected); // Published file stays; temp is gone.
}
