use crate::format::{self, check_size, integrity, ProjectManifest};
use hae_diag::{Code, EngineError, Limits, Result, Stage};
use hae_model::Document;
use hae_package::{sha256_hex, PackageState};
use hae_zip::Archive;
use std::collections::HashSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type FileIdentity = (u64, u64);

// Win32 file attributes/flags. OPEN_REPARSE_POINT protects the final path
// component; std::fs has no handle-relative equivalent for intermediate paths.
// Walking and rechecking ancestors narrows but cannot eliminate the Windows
// path-based TOCTOU window, so this implementation does not claim Unix parity.
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

#[derive(Debug, Clone)]
pub struct InputFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct SaveReceipt {
    pub path: PathBuf,
    pub sha256: String,
    pub revision: u64,
}

#[derive(Debug)]
pub struct OpenedProject {
    pub document: Document,
    pub revision: u64,
    pub manifest: ProjectManifest,
    pub current_hwpx: Arc<[u8]>,
    pub binding: ProjectBinding,
}

#[derive(Debug)]
pub struct ProjectBinding {
    path: PathBuf,
    parent_id: FileIdentity,
    lock_name: String,
    lock_id: FileIdentity,
    lock_file: File,
    file_sha256: String,
    document_id: String,
}

impl ProjectBinding {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_sha256(&self) -> &str {
        &self.file_sha256
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultStep {
    TempWrite,
    TempFsync,
    Rename,
    ParentFsync,
}

#[derive(Clone)]
pub struct IoRoots {
    input_root: PathBuf,
    input_root_id: FileIdentity,
    output_root: PathBuf,
    output_root_id: FileIdentity,
    limits: Limits,
    protected: Arc<Mutex<ProtectedSet>>,
    fault: Arc<Mutex<Option<FaultStep>>>,
}

#[derive(Default)]
struct ProtectedSet {
    paths: HashSet<PathBuf>,
    identities: HashSet<FileIdentity>,
}

struct AnchoredParent {
    parent_path: PathBuf,
    parent_segments: Vec<String>,
    parent_id: FileIdentity,
    name: String,
    canonical: PathBuf,
}

struct PublishOutcome {
    post_publish_problem: Option<String>,
}

fn path_err(stage: Stage, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::PathNotAllowed, stage, message)
}

fn text_too_large(limit: u64, produced: usize) -> EngineError {
    EngineError::new(
        Code::ResourceLimit,
        Stage::Export,
        format!(
            "text export is {produced} bytes, over the limit max_text_export_bytes = {limit} bytes"
        ),
    )
    .with_reason("text_export_too_large")
    .with_detail(serde_json::json!({
        "limit": "max_text_export_bytes",
        "limitBytes": limit,
        "producedBytes": produced,
    }))
}

fn io_err(stage: Stage, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::IoError, stage, message)
}

fn identity_from_path(path: &Path, stage: Stage) -> Result<FileIdentity> {
    let before = fs::symlink_metadata(path).map_err(|error| {
        path_err(
            stage,
            format!("cannot inspect '{}': {error}", path.display()),
        )
    })?;
    if is_reparse_point(&before) || !(before.is_file() || before.is_dir()) {
        return Err(path_err(
            stage,
            format!("cannot identify non-regular path: {}", path.display()),
        ));
    }
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if before.is_dir() {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(flags);
    let file = options
        .open(path)
        .map_err(|error| path_err(stage, format!("cannot open '{}': {error}", path.display())))?;
    let opened = file.metadata().map_err(|error| {
        io_err(
            stage,
            format!("cannot inspect '{}': {error}", path.display()),
        )
    })?;
    if is_reparse_point(&opened) || before.is_dir() != opened.is_dir() {
        return Err(path_err(
            stage,
            format!("'{}' changed type during identity check", path.display()),
        ));
    }
    let id = identity_from_file(&file, stage, &path.display().to_string())?;
    let after = fs::symlink_metadata(path).map_err(|error| {
        path_err(
            stage,
            format!("cannot recheck '{}': {error}", path.display()),
        )
    })?;
    if is_reparse_point(&after) || before.is_dir() != after.is_dir() {
        return Err(path_err(
            stage,
            format!("'{}' changed type during identity check", path.display()),
        ));
    }
    Ok(id)
}

fn identity_from_file(file: &File, stage: Stage, what: &str) -> Result<FileIdentity> {
    let information = winapi_util::file::information(file)
        .map_err(|error| io_err(stage, format!("cannot identify {what}: {error}")))?;
    Ok((information.volume_serial_number(), information.file_index()))
}

fn same_identity(left: &FileIdentity, right: &FileIdentity) -> bool {
    left == right
}

fn check_link_count(file: &File, path: &Path, expected: u64, stage: Stage) -> Result<()> {
    // This is a handle-based snapshot; another process can still add a link
    // after the check because Windows std::fs lacks a directory-handle CAS.
    let information = winapi_util::file::information(file).map_err(|error| {
        io_err(
            stage,
            format!("cannot inspect links for '{}': {error}", path.display()),
        )
    })?;
    if information.number_of_links() != expected {
        return Err(path_err(
            stage,
            format!("expected {expected} link(s): {}", path.display()),
        ));
    }
    Ok(())
}

fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn check_directory(metadata: &Metadata, path: &Path, stage: Stage) -> Result<()> {
    if is_reparse_point(metadata) {
        return Err(path_err(
            stage,
            format!("reparse-point directory is rejected: {}", path.display()),
        ));
    }
    if !metadata.is_dir() {
        return Err(path_err(
            stage,
            format!("not a directory: {}", path.display()),
        ));
    }
    Ok(())
}

fn check_regular(metadata: &Metadata, path: &Path, stage: Stage) -> Result<()> {
    if is_reparse_point(metadata) {
        return Err(path_err(
            stage,
            format!("reparse-point file is rejected: {}", path.display()),
        ));
    }
    if !metadata.is_file() {
        return Err(path_err(
            stage,
            format!("not a regular file: {}", path.display()),
        ));
    }
    Ok(())
}

fn is_reserved_device_name(component: &str) -> bool {
    let base = component.split('.').next().unwrap_or(component);
    let upper = base.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        upper
            .strip_prefix(prefix)
            .is_some_and(|tail| matches!(tail, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
    })
}

fn canonical_segments(path: &Path, stage: Stage) -> Result<Vec<String>> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.is_empty() || wide.contains(&0) {
        return Err(path_err(
            stage,
            "path must be absolute and must not contain NUL",
        ));
    }
    let text =
        String::from_utf16(&wide).map_err(|_| path_err(stage, "path must contain valid UTF-16"))?;
    let bytes = text.as_bytes();
    // Name the rejected form so `C:/data` or a `std::fs::canonicalize` result
    // (`\\?\C:\data`) is not reported as a non-drive path.
    if text.starts_with("\\\\?\\") || text.starts_with("\\\\.\\") {
        return Err(path_err(
            stage,
            "verbatim (\\\\?\\) and device (\\\\.\\) paths are rejected; use a plain drive-letter path (for example C:\\data)",
        ));
    }
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
    {
        return Err(path_err(
            stage,
            "only absolute drive-letter paths (for example C:\\data) are accepted",
        ));
    }
    if text.contains('/') {
        return Err(path_err(
            stage,
            "forward-slash path separators are rejected; use backslashes (for example C:\\data)",
        ));
    }

    let mut segments = vec![format!("{}:", (bytes[0] as char).to_ascii_uppercase())];
    let tail = &text[3..];
    if tail.is_empty() {
        return Ok(segments);
    }
    if tail.ends_with('\\') {
        return Err(path_err(stage, "path must not have a trailing separator"));
    }

    for component in tail.split('\\') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(path_err(
                stage,
                "path must be lexically canonical (no '.', '..', or repeated separators)",
            ));
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(path_err(
                stage,
                "path components ending in a dot or space are rejected",
            ));
        }
        if component.chars().any(|character| {
            character < ' '
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\'
                )
        }) {
            return Err(path_err(
                stage,
                "path component contains a forbidden Windows character or alternate-stream separator",
            ));
        }
        if is_reserved_device_name(component) {
            return Err(path_err(
                stage,
                "reserved Windows device names are rejected",
            ));
        }
        segments.push(component.to_owned());
    }
    Ok(segments)
}

fn path_from_segments(segments: &[String]) -> PathBuf {
    let mut path = PathBuf::from(format!("{}\\", segments[0]));
    for component in &segments[1..] {
        path.push(component);
    }
    path
}

fn inspect_directory(segments: &[String], stage: Stage) -> Result<(PathBuf, FileIdentity)> {
    if segments.is_empty() {
        return Err(path_err(stage, "empty absolute path"));
    }
    let mut path = PathBuf::from(format!("{}\\", segments[0]));
    let mut metadata = fs::symlink_metadata(&path)
        .map_err(|e| path_err(stage, format!("cannot inspect drive root: {e}")))?;
    check_directory(&metadata, &path, stage)?;

    for component in &segments[1..] {
        path.push(component);
        metadata = fs::symlink_metadata(&path).map_err(|e| {
            path_err(
                stage,
                format!(
                    "cannot inspect directory component '{}': {e}",
                    path.display()
                ),
            )
        })?;
        check_directory(&metadata, &path, stage)?;
    }
    let id = identity_from_path(&path, stage)?;
    Ok((path, id))
}

fn validate_parent(parent: &AnchoredParent, stage: Stage) -> Result<()> {
    let (_, current_id) = inspect_directory(&parent.parent_segments, stage)?;
    if !same_identity(&current_id, &parent.parent_id) {
        return Err(path_err(stage, "parent directory identity changed"));
    }
    Ok(())
}

fn sibling(parent: &AnchoredParent, name: String) -> AnchoredParent {
    AnchoredParent {
        parent_path: parent.parent_path.clone(),
        parent_segments: parent.parent_segments.clone(),
        parent_id: parent.parent_id,
        canonical: parent.parent_path.join(&name),
        name,
    }
}

fn target_metadata(
    parent: &AnchoredParent,
    stage: Stage,
) -> Result<Option<(Metadata, FileIdentity)>> {
    validate_parent(parent, stage)?;
    let path = parent.parent_path.join(&parent.name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            check_regular(&metadata, &path, stage)?;
            let (file, id) = open_regular_file(&path, true, false, Some(1), stage)?;
            drop(file);
            Ok(Some((metadata, id)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(path_err(
            stage,
            format!("cannot inspect '{}': {error}", path.display()),
        )),
    }
}

fn open_regular_file(
    path: &Path,
    read: bool,
    write: bool,
    expected_link_count: Option<u64>,
    stage: Stage,
) -> Result<(File, FileIdentity)> {
    let before = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            io_err(stage, format!("file not found: {}", path.display()))
        } else {
            path_err(
                stage,
                format!("cannot inspect '{}': {error}", path.display()),
            )
        }
    })?;
    check_regular(&before, path, stage)?;
    let before_id = identity_from_path(path, stage)?;

    let mut options = OpenOptions::new();
    options
        .read(read)
        .write(write)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options
        .open(path)
        .map_err(|error| path_err(stage, format!("cannot open '{}': {error}", path.display())))?;
    let opened = file
        .metadata()
        .map_err(|error| io_err(stage, format!("cannot inspect open file: {error}")))?;
    check_regular(&opened, path, stage)?;
    if let Some(expected) = expected_link_count {
        check_link_count(&file, path, expected, stage)?;
    }
    let opened_id = identity_from_file(&file, stage, "opened file")?;
    if !same_identity(&opened_id, &before_id) {
        return Err(path_err(
            stage,
            format!("'{}' changed identity during open", path.display()),
        ));
    }
    Ok((file, opened_id))
}

fn verify_regular_path(path: &Path, expected_id: &FileIdentity, stage: Stage) -> Result<Metadata> {
    verify_regular_path_with_links(path, expected_id, Some(1), stage)
}

fn verify_plain_file_path(
    path: &Path,
    expected_id: &FileIdentity,
    stage: Stage,
) -> Result<Metadata> {
    verify_regular_path_with_links(path, expected_id, None, stage)
}

fn verify_regular_path_with_links(
    path: &Path,
    expected_id: &FileIdentity,
    expected_link_count: Option<u64>,
    stage: Stage,
) -> Result<Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        path_err(
            stage,
            format!("cannot inspect '{}': {error}", path.display()),
        )
    })?;
    check_regular(&metadata, path, stage)?;
    let (file, current_id) = open_regular_file(path, true, false, expected_link_count, stage)?;
    if !same_identity(&current_id, expected_id) {
        return Err(path_err(
            stage,
            format!("'{}' changed identity", path.display()),
        ));
    }
    drop(file);
    Ok(metadata)
}

fn cleanup_temp(path: &Path, expected_id: &FileIdentity) {
    if verify_regular_path(path, expected_id, Stage::Save).is_ok() {
        let _ = fs::remove_file(path);
    }
}

fn map_lock_error(error: std::fs::TryLockError, stage: Stage) -> EngineError {
    match error {
        std::fs::TryLockError::WouldBlock => EngineError::new(
            Code::ProjectLocked,
            stage,
            "project is locked by another session",
        ),
        other => io_err(stage, format!("locking project file: {other}")),
    }
}

fn open_or_create_lock(path: &Path, stage: Stage) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    match options.open(path) {
        Ok(file) => {
            let metadata = file
                .metadata()
                .map_err(|error| io_err(stage, format!("inspect new lockfile: {error}")))?;
            check_regular(&metadata, path, stage)?;
            let id = identity_from_file(&file, stage, "new lockfile")?;
            verify_regular_path(path, &id, stage)?;
            Ok(file)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            open_regular_file(path, true, true, Some(1), stage).map(|(file, _)| file)
        }
        Err(error) => Err(path_err(
            stage,
            format!("cannot create lockfile '{}': {error}", path.display()),
        )),
    }
}

fn verify_lock_path(
    path: &Path,
    lock_file: &File,
    expected_id: &FileIdentity,
    stage: Stage,
) -> Result<()> {
    let opened = lock_file
        .metadata()
        .map_err(|_| EngineError::new(Code::ProjectLocked, stage, "bound lock handle invalid"))?;
    check_regular(&opened, path, stage)?;
    check_link_count(lock_file, path, 1, stage)?;
    let opened_id = identity_from_file(lock_file, stage, "bound lock handle")?;
    let (path_file, current_id) =
        open_regular_file(path, true, true, Some(1), stage).map_err(|_| {
            EngineError::new(
                Code::ProjectLocked,
                stage,
                "project lockfile was removed or replaced",
            )
        })?;
    if !same_identity(&opened_id, expected_id) || !same_identity(&current_id, expected_id) {
        return Err(EngineError::new(
            Code::ProjectLocked,
            stage,
            "project lockfile was replaced or unlinked",
        ));
    }
    drop(path_file);
    Ok(())
}

fn acquire_lock(
    parent: &AnchoredParent,
    name: &str,
    stage: Stage,
) -> Result<(String, File, FileIdentity)> {
    validate_parent(parent, stage)?;
    let lock_name = format!("{name}.lock");
    let lock_path = parent.parent_path.join(&lock_name);
    let lock_file = open_or_create_lock(&lock_path, stage)?;
    lock_file
        .try_lock()
        .map_err(|error| map_lock_error(error, stage))?;
    let metadata = lock_file
        .metadata()
        .map_err(|error| io_err(stage, format!("inspect lockfile: {error}")))?;
    check_regular(&metadata, &lock_path, stage)?;
    let id = identity_from_file(&lock_file, stage, "lockfile")?;
    verify_lock_path(&lock_path, &lock_file, &id, stage)?;
    validate_parent(parent, stage)?;
    Ok((lock_name, lock_file, id))
}

fn sync_parent(parent: &AnchoredParent, stage: Stage, inject_failure: bool) -> Result<()> {
    if inject_failure {
        return Err(io_err(stage, "injected parent directory sync failure"));
    }
    validate_parent(parent, stage)?;
    // FlushFileBuffers (File::sync_all) requires GENERIC_WRITE on the handle;
    // a read-only directory handle fails with ERROR_ACCESS_DENIED.
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let directory = options.open(&parent.parent_path).map_err(|error| {
        io_err(
            stage,
            format!("cannot open parent directory for sync: {error}"),
        )
    })?;
    let metadata = directory
        .metadata()
        .map_err(|error| io_err(stage, format!("inspect parent directory: {error}")))?;
    check_directory(&metadata, &parent.parent_path, stage)?;
    let directory_id = identity_from_file(&directory, stage, "parent directory")?;
    if !same_identity(&directory_id, &parent.parent_id) {
        return Err(path_err(
            stage,
            "parent directory identity changed before sync",
        ));
    }
    directory
        .sync_all()
        .map_err(|error| io_err(stage, format!("sync parent directory: {error}")))
}

fn publish_new(
    parent: &AnchoredParent,
    temp_path: &Path,
    target_path: &Path,
    temp_id: &FileIdentity,
    stage: Stage,
) -> Result<PublishOutcome> {
    validate_parent(parent, stage)?;
    verify_regular_path(temp_path, temp_id, stage)?;
    match fs::hard_link(temp_path, target_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(EngineError::new(
                Code::TargetExists,
                stage,
                "target was created concurrently before publication",
            ));
        }
        Err(error) => {
            return Err(io_err(
                stage,
                format!("cannot atomically publish new file: {error}"),
            ));
        }
    }

    // Hard-link publication is the std-only Windows no-replace operation. If
    // cleanup or postconditions fail, publication already happened; callers
    // must update their binding and report durability uncertainty, not roll back.
    let mut problem = None;
    match verify_regular_path_with_links(target_path, temp_id, Some(2), stage) {
        Ok(_) => {}
        Err(error) => problem = Some(format!("published target postcondition failed: {error}")),
    }

    match verify_plain_file_path(temp_path, temp_id, stage) {
        Ok(_) => {
            if let Err(error) = fs::remove_file(temp_path) {
                problem.get_or_insert_with(|| {
                    format!("published temporary link cleanup failed: {error}")
                });
            }
        }
        Err(error) => {
            problem.get_or_insert_with(|| format!("published temporary path changed: {error}"));
        }
    }

    match verify_regular_path(target_path, temp_id, stage) {
        Ok(_) => {}
        Err(error) => {
            problem.get_or_insert_with(|| format!("published target changed: {error}"));
        }
    };
    Ok(PublishOutcome {
        post_publish_problem: problem,
    })
}

fn replace_existing(
    parent: &AnchoredParent,
    temp_path: &Path,
    target_path: &Path,
    temp_id: &FileIdentity,
    stage: Stage,
) -> Result<PublishOutcome> {
    validate_parent(parent, stage)?;
    verify_regular_path(temp_path, temp_id, stage)?;
    fs::rename(temp_path, target_path)
        .map_err(|error| io_err(stage, format!("atomic project replacement failed: {error}")))?;

    let post_publish_problem = match verify_regular_path(target_path, temp_id, stage) {
        Ok(_) => None,
        Err(error) => Some(format!("replaced target postcondition failed: {error}")),
    };
    Ok(PublishOutcome {
        post_publish_problem,
    })
}

fn uncertainty(stage: Stage, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::SaveDurabilityUncertain, stage, message)
}

impl IoRoots {
    pub fn new(
        input_root: impl AsRef<Path>,
        output_root: impl AsRef<Path>,
        limits: Limits,
    ) -> Result<Self> {
        let in_segments = canonical_segments(input_root.as_ref(), Stage::Open)?;
        let out_segments = canonical_segments(output_root.as_ref(), Stage::Open)?;
        let in_path = path_from_segments(&in_segments);
        let out_path = path_from_segments(&out_segments);
        let (_, in_id) = inspect_directory(&in_segments, Stage::Open)?;
        let (_, out_id) = inspect_directory(&out_segments, Stage::Open)?;
        Ok(Self {
            input_root: in_path,
            input_root_id: in_id,
            output_root: out_path,
            output_root_id: out_id,
            limits,
            protected: Arc::new(Mutex::new(ProtectedSet::default())),
            fault: Arc::new(Mutex::new(None)),
        })
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn set_fault(&self, fault: Option<FaultStep>) {
        *self.fault.lock().unwrap() = fault;
    }

    fn take_fault(&self, step: FaultStep) -> bool {
        let mut guard = self.fault.lock().unwrap();
        if *guard == Some(step) {
            *guard = None;
            true
        } else {
            false
        }
    }

    fn resolve_under(
        &self,
        root: &Path,
        root_id: &FileIdentity,
        path: &Path,
        stage: Stage,
    ) -> Result<AnchoredParent> {
        let segments = canonical_segments(path, stage)?;
        let root_segments = canonical_segments(root, stage)?;
        if segments.len() <= root_segments.len()
            || segments[..root_segments.len()] != root_segments[..]
        {
            return Err(path_err(stage, "path is outside configured root"));
        }

        let (_, current_root_id) = inspect_directory(&root_segments, stage)?;
        if !same_identity(&current_root_id, root_id) {
            return Err(path_err(
                stage,
                "configured root directory identity changed",
            ));
        }
        let parent_segments = segments[..segments.len() - 1].to_vec();
        let (parent_path, parent_id) = inspect_directory(&parent_segments, stage)?;
        let name = segments.last().unwrap().clone();
        Ok(AnchoredParent {
            parent_path,
            parent_segments,
            parent_id,
            name,
            canonical: path_from_segments(&segments),
        })
    }

    fn resolve_input(&self, path: &Path, stage: Stage) -> Result<AnchoredParent> {
        self.resolve_under(&self.input_root, &self.input_root_id, path, stage)
    }

    fn resolve_output(&self, path: &Path, stage: Stage) -> Result<AnchoredParent> {
        self.resolve_under(&self.output_root, &self.output_root_id, path, stage)
    }

    fn resolve_project_open(&self, path: &Path, stage: Stage) -> Result<AnchoredParent> {
        if let Ok(resolved) = self.resolve_under(&self.input_root, &self.input_root_id, path, stage)
        {
            return Ok(resolved);
        }
        self.resolve_under(&self.output_root, &self.output_root_id, path, stage)
    }

    fn read_regular_nofollow(
        &self,
        parent: &AnchoredParent,
        max_bytes: u64,
        stage: Stage,
    ) -> Result<(Vec<u8>, FileIdentity)> {
        validate_parent(parent, stage)?;
        let path = parent.parent_path.join(&parent.name);
        let (mut file, id) = open_regular_file(&path, true, false, Some(1), stage)?;
        let metadata = file
            .metadata()
            .map_err(|error| io_err(stage, format!("inspect '{}': {error}", path.display())))?;
        let size = metadata.len();
        if size > max_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                stage,
                "file size limit exceeded",
            ));
        }
        let capacity = usize::try_from(size)
            .map_err(|_| EngineError::new(Code::ResourceLimit, stage, "file is too large"))?;
        let mut bytes = Vec::with_capacity(capacity);
        (&mut file)
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| io_err(stage, format!("read '{}': {error}", path.display())))?;
        if bytes.len() as u64 > max_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                stage,
                "file size limit exceeded",
            ));
        }
        validate_parent(parent, stage)?;
        verify_regular_path(&path, &id, stage)?;
        Ok((bytes, id))
    }

    /// Read an input HWPX or replacement image under `input_root` and register
    /// its path and file identity as protected against subsequent writes.
    pub fn read_input(&self, path: impl AsRef<Path>) -> Result<InputFile> {
        let resolved = self.resolve_input(path.as_ref(), Stage::Open)?;
        let (bytes, id) =
            self.read_regular_nofollow(&resolved, self.limits.max_hwpx_bytes, Stage::Open)?;
        let sha256 = sha256_hex(&bytes);
        let mut protected = self.protected.lock().unwrap();
        protected.paths.insert(resolved.canonical.clone());
        protected.identities.insert(id);
        Ok(InputFile {
            path: resolved.canonical,
            bytes,
            sha256,
        })
    }

    fn check_not_protected(
        &self,
        canonical: &Path,
        existing_id: Option<FileIdentity>,
        stage: Stage,
    ) -> Result<()> {
        let protected = self.protected.lock().unwrap();
        if protected.paths.contains(canonical) {
            return Err(path_err(stage, "cannot overwrite protected input path"));
        }
        if let Some(id) = existing_id {
            if protected.identities.contains(&id) {
                return Err(path_err(
                    stage,
                    "cannot overwrite protected input file identity",
                ));
            }
        }
        Ok(())
    }

    fn verify_bound_lock(
        parent: &AnchoredParent,
        binding: &ProjectBinding,
        stage: Stage,
    ) -> Result<()> {
        validate_parent(parent, stage)?;
        let lock_path = parent.parent_path.join(&binding.lock_name);
        verify_lock_path(&lock_path, &binding.lock_file, &binding.lock_id, stage)
    }

    /// Open a `.hwpwork` project: acquire its session lock BEFORE reading or
    /// validating integrity, then decode and verify all hashes/relations.
    pub fn open_project(&self, path: impl AsRef<Path>) -> Result<OpenedProject> {
        let resolved = self.resolve_project_open(path.as_ref(), Stage::Open)?;
        let (lock_name, lock_file, lock_id) = acquire_lock(&resolved, &resolved.name, Stage::Open)?;
        let (bytes, _) =
            self.read_regular_nofollow(&resolved, self.limits.max_project_bytes, Stage::Open)?;
        let file_sha256 = sha256_hex(&bytes);
        let decoded = format::decode(bytes, &self.limits)?;
        let binding = ProjectBinding {
            path: resolved.canonical,
            parent_id: resolved.parent_id,
            lock_name,
            lock_id,
            lock_file,
            file_sha256,
            document_id: decoded.document.document_id.clone(),
        };
        Ok(OpenedProject {
            document: decoded.document,
            revision: decoded.manifest.revision,
            manifest: decoded.manifest,
            current_hwpx: decoded.current,
            binding,
        })
    }

    fn write_temp_file(
        &self,
        parent: &AnchoredParent,
        base_name: &str,
        payload: &[u8],
        stage: Stage,
    ) -> Result<(String, File, FileIdentity)> {
        validate_parent(parent, stage)?;
        let rand_id =
            hae_model::new_id("tmp-").map_err(|error| io_err(stage, error.to_string()))?;
        let temp_name = format!(".{base_name}.{rand_id}");
        let temp_path = parent.parent_path.join(&temp_name);
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let mut file = options
            .open(&temp_path)
            .map_err(|error| io_err(stage, format!("create temp file: {error}")))?;
        let metadata = file
            .metadata()
            .map_err(|error| io_err(stage, format!("inspect temp file: {error}")))?;
        check_regular(&metadata, &temp_path, stage)?;
        check_link_count(&file, &temp_path, 1, stage)?;
        let temp_id = identity_from_file(&file, stage, "temporary file")?;

        let write_result = if self.take_fault(FaultStep::TempWrite) {
            Err(std::io::Error::other("injected temp write failure"))
        } else {
            file.write_all(payload)
        };
        if let Err(error) = write_result {
            drop(file);
            cleanup_temp(&temp_path, &temp_id);
            return Err(io_err(stage, format!("write temp file: {error}")));
        }
        verify_regular_path(&temp_path, &temp_id, stage)?;
        Ok((temp_name, file, temp_id))
    }

    /// Save a `.hwpwork` project atomically.
    pub fn save_project<F>(
        &self,
        binding: &mut Option<ProjectBinding>,
        path: impl AsRef<Path>,
        doc: &Document,
        revision: u64,
        validate: F,
    ) -> Result<SaveReceipt>
    where
        F: FnOnce(&[u8]) -> Result<()>,
    {
        let resolved = self.resolve_output(path.as_ref(), Stage::Save)?;
        let existing = target_metadata(&resolved, Stage::Save)?;
        let existing_id = existing.as_ref().map(|(_, id)| *id);
        self.check_not_protected(&resolved.canonical, existing_id, Stage::Save)?;

        enum ModeState<'a> {
            First {
                lock_name: String,
                lock_file: File,
                lock_id: FileIdentity,
            },
            Bound(&'a mut ProjectBinding),
        }

        let mode = match binding.as_mut() {
            None => {
                let (lock_name, lock_file, lock_id) =
                    acquire_lock(&resolved, &resolved.name, Stage::Save)?;
                if target_metadata(&resolved, Stage::Save)?.is_some() {
                    return Err(EngineError::new(
                        Code::TargetExists,
                        Stage::Save,
                        "first project.save requires a non-existent target path",
                    ));
                }
                ModeState::First {
                    lock_name,
                    lock_file,
                    lock_id,
                }
            }
            Some(current) => {
                if current.path != resolved.canonical || current.parent_id != resolved.parent_id {
                    return Err(path_err(
                        Stage::Save,
                        "session is already bound to a different project path",
                    ));
                }
                if current.document_id != doc.document_id {
                    return Err(EngineError::new(
                        Code::SessionMismatch,
                        Stage::Save,
                        "documentId does not match bound project",
                    ));
                }
                Self::verify_bound_lock(&resolved, current, Stage::Save)?;
                let (disk_bytes, _) = self
                    .read_regular_nofollow(&resolved, self.limits.max_project_bytes, Stage::Save)
                    .map_err(|error| {
                        if error.code() == Code::IoError {
                            EngineError::new(
                                Code::ExternalModification,
                                Stage::Save,
                                "bound project file was removed or unreadable",
                            )
                        } else {
                            error
                        }
                    })?;
                if sha256_hex(&disk_bytes) != current.file_sha256 {
                    return Err(EngineError::new(
                        Code::ExternalModification,
                        Stage::Save,
                        "project file on disk was modified outside this session",
                    ));
                }
                ModeState::Bound(current)
            }
        };

        let encoded = format::encode(doc, revision, &self.limits)?;
        let new_sha256 = sha256_hex(&encoded);
        let (temp_name, temp_file, temp_id) =
            self.write_temp_file(&resolved, &resolved.name, &encoded, Stage::Save)?;
        let temp = sibling(&resolved, temp_name.clone());
        let temp_path = temp.canonical.clone();
        let target_path = resolved.canonical.clone();
        let mut post_publish_problem = None;

        let pre_publish = (|| -> Result<()> {
            let (reread, reread_id) =
                self.read_regular_nofollow(&temp, self.limits.max_project_bytes, Stage::Save)?;
            if reread_id != temp_id || reread != encoded {
                return Err(integrity("temporary project identity or bytes changed"));
            }
            let decoded = format::decode(reread, &self.limits)?;
            validate(&decoded.current)?;
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(Stage::Save, "injected temp fsync failure"));
            }
            temp_file
                .sync_all()
                .map_err(|error| io_err(Stage::Save, format!("sync temp project: {error}")))?;
            validate_parent(&resolved, Stage::Save)?;

            match &mode {
                ModeState::First {
                    lock_name,
                    lock_file,
                    lock_id,
                } => {
                    let lock_path = resolved.parent_path.join(lock_name);
                    verify_lock_path(&lock_path, lock_file, lock_id, Stage::Save)?;
                    if target_metadata(&resolved, Stage::Save)?.is_some() {
                        return Err(EngineError::new(
                            Code::TargetExists,
                            Stage::Save,
                            "project target was created before publication",
                        ));
                    }
                    if self.take_fault(FaultStep::Rename) {
                        return Err(io_err(Stage::Save, "injected rename failure"));
                    }
                    post_publish_problem =
                        publish_new(&resolved, &temp_path, &target_path, &temp_id, Stage::Save)?
                            .post_publish_problem;
                }
                ModeState::Bound(current) => {
                    Self::verify_bound_lock(&resolved, current, Stage::Save)?;
                    let (disk_bytes, disk_id) = self
                        .read_regular_nofollow(
                            &resolved,
                            self.limits.max_project_bytes,
                            Stage::Save,
                        )
                        .map_err(|_| {
                            EngineError::new(
                                Code::ExternalModification,
                                Stage::Save,
                                "bound project file removed before replacement",
                            )
                        })?;
                    self.check_not_protected(&resolved.canonical, Some(disk_id), Stage::Save)?;
                    if sha256_hex(&disk_bytes) != current.file_sha256 {
                        return Err(EngineError::new(
                            Code::ExternalModification,
                            Stage::Save,
                            "project file modified concurrently before replacement",
                        ));
                    }
                    if self.take_fault(FaultStep::Rename) {
                        return Err(io_err(Stage::Save, "injected rename failure"));
                    }
                    post_publish_problem = replace_existing(
                        &resolved,
                        &temp_path,
                        &target_path,
                        &temp_id,
                        Stage::Save,
                    )?
                    .post_publish_problem;
                }
            }
            Ok(())
        })();

        if let Err(error) = pre_publish {
            drop(temp_file);
            cleanup_temp(&temp_path, &temp_id);
            return Err(error);
        }
        drop(temp_file);

        match mode {
            ModeState::First {
                lock_name,
                lock_file,
                lock_id,
            } => {
                *binding = Some(ProjectBinding {
                    path: resolved.canonical.clone(),
                    parent_id: resolved.parent_id,
                    lock_name,
                    lock_id,
                    lock_file,
                    file_sha256: new_sha256.clone(),
                    document_id: doc.document_id.clone(),
                });
            }
            ModeState::Bound(current) => {
                current.file_sha256 = new_sha256.clone();
            }
        }

        if let Some(problem) = post_publish_problem {
            return Err(uncertainty(
                Stage::Save,
                format!("project published but Windows postcondition failed: {problem}"),
            ));
        }
        if let Err(error) = sync_parent(
            &resolved,
            Stage::Save,
            self.take_fault(FaultStep::ParentFsync),
        ) {
            return Err(uncertainty(
                Stage::Save,
                format!("project published but parent directory sync failed: {error}"),
            ));
        }

        Ok(SaveReceipt {
            path: resolved.canonical,
            sha256: new_sha256,
            revision,
        })
    }

    /// Publish caller-validated PNG/SVG bytes directly under `output_root`.
    pub fn write_preview_artifact(
        &self,
        extension: &str,
        payload: &[u8],
        revision: u64,
    ) -> Result<SaveReceipt> {
        let stage = Stage::Render;
        if !matches!(extension, "png" | "svg") {
            return Err(EngineError::new(
                Code::InvalidRequest,
                stage,
                "preview artifact extension must be 'png' or 'svg'",
            ));
        }
        let max_bytes = self
            .limits
            .max_project_bytes
            .min(self.limits.max_session_memory / 2);
        if payload.len() as u64 > max_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                stage,
                "preview artifact exceeds project or readback memory limit",
            ));
        }

        let id = hae_model::new_id("preview-").map_err(|error| io_err(stage, error.to_string()))?;
        let path = self.output_root.join(format!("{id}.{extension}"));
        let resolved = self.resolve_output(&path, stage)?;
        if !same_identity(&resolved.parent_id, &self.output_root_id) {
            return Err(path_err(stage, "configured output root identity changed"));
        }
        self.check_not_protected(&resolved.canonical, None, stage)?;
        if target_metadata(&resolved, stage)?.is_some() {
            return Err(EngineError::new(
                Code::TargetExists,
                stage,
                "preview artifact target already exists",
            ));
        }

        let sha256 = sha256_hex(payload);
        let (temp_name, temp_file, temp_id) =
            self.write_temp_file(&resolved, &resolved.name, payload, stage)?;
        let temp = sibling(&resolved, temp_name);
        let temp_path = temp.canonical.clone();
        let target_path = resolved.canonical.clone();
        let mut post_publish_problem = None;
        let pre_publish = (|| -> Result<()> {
            let (reread, reread_id) = self.read_regular_nofollow(&temp, max_bytes, stage)?;
            if reread_id != temp_id || reread != payload || sha256_hex(&reread) != sha256 {
                return Err(io_err(stage, "temporary preview identity or bytes changed"));
            }
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(stage, "injected temp fsync failure"));
            }
            temp_file
                .sync_all()
                .map_err(|error| io_err(stage, format!("sync temp preview: {error}")))?;
            validate_parent(&resolved, stage)?;
            self.check_not_protected(&resolved.canonical, None, stage)?;
            if self.take_fault(FaultStep::Rename) {
                return Err(io_err(stage, "injected rename failure"));
            }
            post_publish_problem =
                publish_new(&resolved, &temp_path, &target_path, &temp_id, stage)?
                    .post_publish_problem;
            Ok(())
        })();
        if let Err(error) = pre_publish {
            drop(temp_file);
            cleanup_temp(&temp_path, &temp_id);
            return Err(error);
        }
        drop(temp_file);

        let receipt = SaveReceipt {
            path: resolved.canonical.clone(),
            sha256,
            revision,
        };
        if let Some(problem) = post_publish_problem {
            return Err(uncertainty(
                stage,
                format!("preview published but Windows postcondition failed: {problem}"),
            )
            .with_detail(serde_json::json!({
                "path": receipt.path,
                "sha256": receipt.sha256,
                "revision": receipt.revision,
            })));
        }
        if let Err(error) = sync_parent(&resolved, stage, self.take_fault(FaultStep::ParentFsync)) {
            return Err(uncertainty(
                stage,
                format!("preview artifact published but parent directory sync failed: {error}"),
            )
            .with_detail(serde_json::json!({
                "path": receipt.path,
                "sha256": receipt.sha256,
                "revision": receipt.revision,
            })));
        }
        Ok(receipt)
    }

    /// Export a `.hwpx` package to a new file under `output_root`.
    /// Never overwrites existing files or protected inputs.
    pub fn export_hwpx<F>(
        &self,
        path: impl AsRef<Path>,
        doc: &Document,
        validate: F,
    ) -> Result<SaveReceipt>
    where
        F: FnOnce(&[u8]) -> Result<()>,
    {
        let stage = Stage::Export;
        let resolved = self.resolve_output(path.as_ref(), stage)?;
        let existing = target_metadata(&resolved, stage)?;
        self.check_not_protected(
            &resolved.canonical,
            existing.as_ref().map(|(_, id)| *id),
            stage,
        )?;
        if existing.is_some() {
            return Err(EngineError::new(
                Code::TargetExists,
                stage,
                "export target already exists",
            ));
        }

        let bytes = format::export_bytes(doc, &self.limits)?;
        check_size(bytes.len(), self.limits.max_hwpx_bytes)?;
        let sha256 = sha256_hex(&bytes);
        let (temp_name, temp_file, temp_id) =
            self.write_temp_file(&resolved, &resolved.name, &bytes, stage)?;
        let temp = sibling(&resolved, temp_name);
        let temp_path = temp.canonical.clone();
        let target_path = resolved.canonical.clone();
        let mut post_publish_problem = None;
        let pre_publish = (|| -> Result<()> {
            let (reread, reread_id) =
                self.read_regular_nofollow(&temp, self.limits.max_hwpx_bytes, stage)?;
            if reread_id != temp_id || reread != bytes {
                return Err(io_err(stage, "temporary export identity or bytes changed"));
            }
            let archive = Archive::open(reread.clone(), &self.limits)?;
            let state = PackageState::from_archive(&archive);
            hae_package::analyze(
                &state,
                archive
                    .entries()
                    .first()
                    .is_some_and(|entry| entry.is_stored()),
                &self.limits,
            )?;
            validate(&reread)?;
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(stage, "injected temp fsync failure"));
            }
            temp_file
                .sync_all()
                .map_err(|error| io_err(stage, format!("sync temp export: {error}")))?;
            validate_parent(&resolved, stage)?;
            if self.take_fault(FaultStep::Rename) {
                return Err(io_err(stage, "injected rename failure"));
            }
            post_publish_problem =
                publish_new(&resolved, &temp_path, &target_path, &temp_id, stage)?
                    .post_publish_problem;
            Ok(())
        })();
        if let Err(error) = pre_publish {
            drop(temp_file);
            cleanup_temp(&temp_path, &temp_id);
            return Err(error);
        }
        drop(temp_file);

        let receipt = SaveReceipt {
            path: resolved.canonical.clone(),
            sha256,
            revision: 0,
        };
        if let Some(problem) = post_publish_problem {
            return Err(uncertainty(
                stage,
                format!("export published but Windows postcondition failed: {problem}"),
            ));
        }
        if let Err(error) = sync_parent(&resolved, stage, self.take_fault(FaultStep::ParentFsync)) {
            return Err(uncertainty(
                stage,
                format!("export published but parent directory sync failed: {error}"),
            ));
        }
        Ok(receipt)
    }

    /// Publish Markdown/HTML text as a new file under `output_root`.
    /// Never overwrites existing files or protected inputs; the size is
    /// bounded by `max_text_export_bytes`.
    pub fn export_text(
        &self,
        path: impl AsRef<Path>,
        bytes: &[u8],
        revision: u64,
    ) -> Result<SaveReceipt> {
        let stage = Stage::Export;
        let max = self.limits.max_text_export_bytes;
        if bytes.len() as u64 > max {
            return Err(text_too_large(max, bytes.len()));
        }
        let resolved = self.resolve_output(path.as_ref(), stage)?;
        let existing = target_metadata(&resolved, stage)?;
        self.check_not_protected(
            &resolved.canonical,
            existing.as_ref().map(|(_, id)| *id),
            stage,
        )?;
        if existing.is_some() {
            return Err(EngineError::new(
                Code::TargetExists,
                stage,
                "export target already exists",
            ));
        }

        let sha256 = sha256_hex(bytes);
        let (temp_name, temp_file, temp_id) =
            self.write_temp_file(&resolved, &resolved.name, bytes, stage)?;
        let temp = sibling(&resolved, temp_name);
        let temp_path = temp.canonical.clone();
        let target_path = resolved.canonical.clone();
        let mut post_publish_problem = None;
        let pre_publish = (|| -> Result<()> {
            let (reread, reread_id) = self.read_regular_nofollow(&temp, max, stage)?;
            if reread_id != temp_id || reread != bytes {
                return Err(io_err(stage, "temporary export identity or bytes changed"));
            }
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(stage, "injected temp fsync failure"));
            }
            temp_file
                .sync_all()
                .map_err(|error| io_err(stage, format!("sync temp export: {error}")))?;
            validate_parent(&resolved, stage)?;
            if self.take_fault(FaultStep::Rename) {
                return Err(io_err(stage, "injected rename failure"));
            }
            post_publish_problem =
                publish_new(&resolved, &temp_path, &target_path, &temp_id, stage)?
                    .post_publish_problem;
            Ok(())
        })();
        if let Err(error) = pre_publish {
            drop(temp_file);
            cleanup_temp(&temp_path, &temp_id);
            return Err(error);
        }
        drop(temp_file);

        let receipt = SaveReceipt {
            path: resolved.canonical.clone(),
            sha256,
            revision,
        };
        if let Some(problem) = post_publish_problem {
            return Err(uncertainty(
                stage,
                format!("text export published but Windows postcondition failed: {problem}"),
            ));
        }
        if let Err(error) = sync_parent(&resolved, stage, self.take_fault(FaultStep::ParentFsync)) {
            return Err(uncertainty(
                stage,
                format!("text export published but parent directory sync failed: {error}"),
            ));
        }
        Ok(receipt)
    }
}
