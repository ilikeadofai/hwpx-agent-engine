use crate::format::{self, check_size, integrity, ProjectManifest};
use hae_diag::{Code, EngineError, Limits, Result, Stage};
use hae_model::Document;
use hae_package::{sha256_hex, PackageState};
use hae_zip::Archive;
use rustix::fd::OwnedFd;
use rustix::fs::{
    flock, fstat, fsync, openat, renameat, renameat_with, statat, unlinkat, AtFlags,
    FlockOperation, Mode, OFlags, RenameFlags, Stat, CWD,
};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;

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
    parent_id: (u64, u64),
    lock_name: String,
    lock_id: (u64, u64),
    lock_fd: OwnedFd,
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
    input_root_id: (u64, u64),
    output_root: PathBuf,
    output_root_id: (u64, u64),
    limits: Limits,
    protected: Arc<Mutex<ProtectedSet>>,
    fault: Arc<Mutex<Option<FaultStep>>>,
}

#[derive(Default)]
struct ProtectedSet {
    paths: HashSet<PathBuf>,
    inodes: HashSet<(u64, u64)>,
}

fn stat_id(st: &Stat) -> (u64, u64) {
    #[allow(clippy::useless_conversion)]
    (u64::from(st.st_dev), u64::from(st.st_ino))
}

fn is_reg(st: &Stat) -> bool {
    #[allow(clippy::useless_conversion)]
    let mode = u32::from(st.st_mode);
    (mode & S_IFMT) == S_IFREG
}

fn is_dir(st: &Stat) -> bool {
    #[allow(clippy::useless_conversion)]
    let mode = u32::from(st.st_mode);
    (mode & S_IFMT) == S_IFDIR
}

fn path_err(stage: Stage, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::PathNotAllowed, stage, message)
}

fn io_err(stage: Stage, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::IoError, stage, message)
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

fn canonical_segments(path: &Path, stage: Stage) -> Result<Vec<String>> {
    let raw = path.as_os_str().as_bytes();
    if raw.is_empty() || raw[0] != b'/' || raw.contains(&0) {
        return Err(path_err(stage, "path must be absolute without NUL bytes"));
    }
    let s = std::str::from_utf8(raw).map_err(|_| path_err(stage, "path must be valid UTF-8"))?;
    if s == "/" {
        return Ok(Vec::new());
    }
    if s.ends_with('/') {
        return Err(path_err(stage, "path must not have a trailing slash"));
    }
    let mut out = Vec::new();
    for part in &s[1..].split('/').collect::<Vec<_>>() {
        if part.is_empty() || *part == "." || *part == ".." {
            return Err(path_err(
                stage,
                "path must be lexically canonical (no '.', '..', or '//')",
            ));
        }
        out.push((*part).to_string());
    }
    Ok(out)
}

fn open_dir_nofollow(segments: &[String], stage: Stage) -> Result<(OwnedFd, (u64, u64))> {
    let mut dir = openat(
        CWD,
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| path_err(stage, format!("cannot open /: {e}")))?;
    let mut st = fstat(&dir).map_err(|e| io_err(stage, format!("fstat /: {e}")))?;
    if !is_dir(&st) {
        return Err(path_err(stage, "/ is not a directory"));
    }
    for seg in segments {
        dir = openat(
            &dir,
            seg.as_str(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| {
            path_err(
                stage,
                format!("directory traversal rejected at '{seg}': {e}"),
            )
        })?;
        st = fstat(&dir).map_err(|e| io_err(stage, format!("fstat '{seg}': {e}")))?;
        if !is_dir(&st) {
            return Err(path_err(stage, format!("'{seg}' is not a directory")));
        }
    }
    Ok((dir, stat_id(&st)))
}

struct AnchoredParent {
    dir: OwnedFd,
    parent_id: (u64, u64),
    name: String,
    canonical: PathBuf,
}

impl IoRoots {
    pub fn new(
        input_root: impl AsRef<Path>,
        output_root: impl AsRef<Path>,
        limits: Limits,
    ) -> Result<Self> {
        let in_path = input_root.as_ref().to_path_buf();
        let out_path = output_root.as_ref().to_path_buf();
        let in_segs = canonical_segments(&in_path, Stage::Open)?;
        let out_segs = canonical_segments(&out_path, Stage::Open)?;
        let (_, in_id) = open_dir_nofollow(&in_segs, Stage::Open)?;
        let (_, out_id) = open_dir_nofollow(&out_segs, Stage::Open)?;
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
        let mut g = self.fault.lock().unwrap();
        if *g == Some(step) {
            *g = None;
            true
        } else {
            false
        }
    }

    fn resolve_under(
        &self,
        root: &Path,
        root_id: (u64, u64),
        path: &Path,
        stage: Stage,
    ) -> Result<AnchoredParent> {
        let segs = canonical_segments(path, stage)?;
        let root_segs = canonical_segments(root, stage)?;
        if segs.len() <= root_segs.len() || segs[..root_segs.len()] != root_segs[..] {
            return Err(path_err(stage, "path is outside configured root"));
        }
        let (_, current_root_id) = open_dir_nofollow(&root_segs, stage)?;
        if current_root_id != root_id {
            return Err(path_err(
                stage,
                "configured root directory identity changed",
            ));
        }
        let parent_segs = &segs[..segs.len() - 1];
        let (dir, parent_id) = open_dir_nofollow(parent_segs, stage)?;
        let name = segs.last().unwrap().clone();
        Ok(AnchoredParent {
            dir,
            parent_id,
            name,
            canonical: path.to_path_buf(),
        })
    }

    fn resolve_input(&self, path: &Path, stage: Stage) -> Result<AnchoredParent> {
        self.resolve_under(&self.input_root, self.input_root_id, path, stage)
    }

    fn resolve_output(&self, path: &Path, stage: Stage) -> Result<AnchoredParent> {
        self.resolve_under(&self.output_root, self.output_root_id, path, stage)
    }

    fn resolve_project_open(&self, path: &Path, stage: Stage) -> Result<AnchoredParent> {
        if let Ok(a) = self.resolve_under(&self.input_root, self.input_root_id, path, stage) {
            return Ok(a);
        }
        self.resolve_under(&self.output_root, self.output_root_id, path, stage)
    }

    fn read_regular_nofollow(
        dir: &OwnedFd,
        name: &str,
        max_bytes: u64,
        stage: Stage,
    ) -> Result<(Vec<u8>, (u64, u64))> {
        let lstat = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|e| {
            if e == rustix::io::Errno::NOENT {
                io_err(stage, format!("file not found: {name}"))
            } else {
                path_err(stage, format!("cannot stat '{name}': {e}"))
            }
        })?;
        if !is_reg(&lstat) {
            return Err(path_err(stage, format!("'{name}' is not a regular file")));
        }
        #[allow(clippy::useless_conversion)]
        if u64::from(lstat.st_nlink) != 1 {
            return Err(path_err(
                stage,
                format!("hard links are rejected: '{name}'"),
            ));
        }
        let fd = openat(
            dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| path_err(stage, format!("cannot open '{name}': {e}")))?;
        let st = fstat(&fd).map_err(|e| io_err(stage, format!("fstat '{name}': {e}")))?;
        if !is_reg(&st) || stat_id(&st) != stat_id(&lstat) {
            return Err(path_err(
                stage,
                format!("'{name}' changed identity during open"),
            ));
        }
        #[allow(clippy::useless_conversion)]
        if u64::from(st.st_nlink) != 1 {
            return Err(path_err(
                stage,
                format!("hard links are rejected: '{name}'"),
            ));
        }
        let size = u64::try_from(st.st_size).map_err(|_| io_err(stage, "negative file size"))?;
        if size > max_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                stage,
                "file size limit exceeded",
            ));
        }
        let mut file = std::fs::File::from(fd);
        let mut buf = Vec::with_capacity(size as usize);
        (&mut file)
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut buf)
            .map_err(|e| io_err(stage, format!("read '{name}': {e}")))?;
        if buf.len() as u64 > max_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                stage,
                "file size limit exceeded",
            ));
        }
        Ok((buf, stat_id(&st)))
    }

    /// Read an input HWPX or replacement image under `input_root` and register
    /// its path and `(dev, ino)` as protected against subsequent writes.
    pub fn read_input(&self, path: impl AsRef<Path>) -> Result<InputFile> {
        let a = self.resolve_input(path.as_ref(), Stage::Open)?;
        let (bytes, id) =
            Self::read_regular_nofollow(&a.dir, &a.name, self.limits.max_hwpx_bytes, Stage::Open)?;
        let sha256 = sha256_hex(&bytes);
        let mut prot = self.protected.lock().unwrap();
        prot.paths.insert(a.canonical.clone());
        prot.inodes.insert(id);
        Ok(InputFile {
            path: a.canonical,
            bytes,
            sha256,
        })
    }

    fn check_not_protected(
        &self,
        canonical: &Path,
        existing_id: Option<(u64, u64)>,
        stage: Stage,
    ) -> Result<()> {
        let prot = self.protected.lock().unwrap();
        if prot.paths.contains(canonical) {
            return Err(path_err(stage, "cannot overwrite protected input path"));
        }
        if let Some(id) = existing_id {
            if prot.inodes.contains(&id) {
                return Err(path_err(stage, "cannot overwrite protected input inode"));
            }
        }
        Ok(())
    }

    fn acquire_lock(
        dir: &OwnedFd,
        name: &str,
        stage: Stage,
    ) -> Result<(String, OwnedFd, (u64, u64))> {
        let lock_name = format!("{name}.lock");
        let fd = openat(
            dir,
            lock_name.as_str(),
            OFlags::CREATE | OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| path_err(stage, format!("cannot open lockfile '{lock_name}': {e}")))?;
        let fst = fstat(&fd).map_err(|e| io_err(stage, format!("fstat lockfile: {e}")))?;
        #[allow(clippy::useless_conversion)]
        if !is_reg(&fst) || u64::from(fst.st_nlink) != 1 {
            return Err(path_err(
                stage,
                "lockfile must be a single-link regular file",
            ));
        }
        flock(&fd, FlockOperation::NonBlockingLockExclusive).map_err(|e| {
            if e == rustix::io::Errno::WOULDBLOCK {
                EngineError::new(
                    Code::ProjectLocked,
                    stage,
                    "project is locked by another session",
                )
            } else {
                io_err(stage, format!("flock '{lock_name}': {e}"))
            }
        })?;
        let lst = statat(dir, lock_name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|_| {
            EngineError::new(
                Code::ProjectLocked,
                stage,
                "lockfile disappeared during lock acquisition",
            )
        })?;
        #[allow(clippy::useless_conversion)]
        if !is_reg(&lst) || u64::from(lst.st_nlink) != 1 || stat_id(&lst) != stat_id(&fst) {
            return Err(EngineError::new(
                Code::ProjectLocked,
                stage,
                "lockfile was replaced during lock acquisition",
            ));
        }
        Ok((lock_name, fd, stat_id(&fst)))
    }

    fn verify_bound_lock(dir: &OwnedFd, b: &ProjectBinding, stage: Stage) -> Result<()> {
        let fst = fstat(&b.lock_fd).map_err(|_| {
            EngineError::new(Code::ProjectLocked, stage, "bound lock descriptor invalid")
        })?;
        let lst = statat(dir, b.lock_name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|_| {
            EngineError::new(Code::ProjectLocked, stage, "project lockfile was removed")
        })?;
        #[allow(clippy::useless_conversion)]
        if !is_reg(&lst)
            || u64::from(lst.st_nlink) != 1
            || u64::from(fst.st_nlink) != 1
            || stat_id(&fst) != b.lock_id
            || stat_id(&lst) != b.lock_id
        {
            return Err(EngineError::new(
                Code::ProjectLocked,
                stage,
                "project lockfile was replaced or unlinked",
            ));
        }
        Ok(())
    }

    /// Open a `.hwpwork` project: acquire its session lock BEFORE reading or
    /// validating integrity, then decode and verify all hashes/relations.
    pub fn open_project(&self, path: impl AsRef<Path>) -> Result<OpenedProject> {
        let a = self.resolve_project_open(path.as_ref(), Stage::Open)?;
        let (lock_name, lock_fd, lock_id) = Self::acquire_lock(&a.dir, &a.name, Stage::Open)?;
        let (bytes, _) = Self::read_regular_nofollow(
            &a.dir,
            &a.name,
            self.limits.max_project_bytes,
            Stage::Open,
        )?;
        let file_sha256 = sha256_hex(&bytes);
        let decoded = format::decode(bytes, &self.limits)?;
        let binding = ProjectBinding {
            path: a.canonical,
            parent_id: a.parent_id,
            lock_name,
            lock_id,
            lock_fd,
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
        dir: &OwnedFd,
        base_name: &str,
        payload: &[u8],
        stage: Stage,
    ) -> Result<(String, OwnedFd)> {
        let rand_id = hae_model::new_id("tmp-").map_err(|e| io_err(stage, e.to_string()))?;
        let tmp_name = format!(".{base_name}.{rand_id}");
        let fd = openat(
            dir,
            tmp_name.as_str(),
            OFlags::CREATE | OFlags::EXCL | OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| io_err(stage, format!("create temp file '{tmp_name}': {e}")))?;
        let mut file = std::fs::File::from(fd);
        let write_res = if self.take_fault(FaultStep::TempWrite) {
            Err(std::io::Error::other("injected temp write failure"))
        } else {
            file.write_all(payload)
        };
        if let Err(e) = write_res {
            let _ = unlinkat(dir, tmp_name.as_str(), AtFlags::empty());
            return Err(io_err(stage, format!("write temp file: {e}")));
        }
        let fd = OwnedFd::from(file);
        Ok((tmp_name, fd))
    }

    /// Save a `.hwpwork` project atomically.
    ///
    /// On first save (`binding.is_none()`), acquires `<name>.lock`, verifies
    /// target does not exist, validates the extracted `current.hwpx` via
    /// `validate`, fsyncs temp file, publishes with `RENAME_NOREPLACE`, binds
    /// `binding`, and fsyncs the parent directory.
    ///
    /// On subsequent saves (`binding.is_some()`), verifies the bound path,
    /// lock inode, and on-disk `file_sha256` (`EXTERNAL_MODIFICATION` on
    /// mismatch), then atomically replaces the project file.
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
        let a = self.resolve_output(path.as_ref(), Stage::Save)?;
        let existing_stat = statat(&a.dir, a.name.as_str(), AtFlags::SYMLINK_NOFOLLOW).ok();
        if let Some(st) = &existing_stat {
            if !is_reg(st) {
                return Err(path_err(Stage::Save, "target is not a regular file"));
            }
            #[allow(clippy::useless_conversion)]
            if u64::from(st.st_nlink) != 1 {
                return Err(path_err(Stage::Save, "target has hard links"));
            }
        }
        self.check_not_protected(
            &a.canonical,
            existing_stat.as_ref().map(stat_id),
            Stage::Save,
        )?;

        enum ModeState<'a> {
            First {
                lock_name: String,
                lock_fd: OwnedFd,
                lock_id: (u64, u64),
            },
            Bound(&'a mut ProjectBinding),
        }

        let mode = match binding.as_mut() {
            None => {
                let (lock_name, lock_fd, lock_id) =
                    Self::acquire_lock(&a.dir, &a.name, Stage::Save)?;
                if statat(&a.dir, a.name.as_str(), AtFlags::SYMLINK_NOFOLLOW).is_ok() {
                    return Err(EngineError::new(
                        Code::TargetExists,
                        Stage::Save,
                        "first project.save requires a non-existent target path",
                    ));
                }
                ModeState::First {
                    lock_name,
                    lock_fd,
                    lock_id,
                }
            }
            Some(b) => {
                if b.path != a.canonical || b.parent_id != a.parent_id {
                    return Err(path_err(
                        Stage::Save,
                        "session is already bound to a different project path",
                    ));
                }
                if b.document_id != doc.document_id {
                    return Err(EngineError::new(
                        Code::SessionMismatch,
                        Stage::Save,
                        "documentId does not match bound project",
                    ));
                }
                Self::verify_bound_lock(&a.dir, b, Stage::Save)?;
                let (disk_bytes, _) = Self::read_regular_nofollow(
                    &a.dir,
                    &a.name,
                    self.limits.max_project_bytes,
                    Stage::Save,
                )
                .map_err(|e| {
                    if e.code() == Code::IoError {
                        EngineError::new(
                            Code::ExternalModification,
                            Stage::Save,
                            "bound project file was removed or unreadable",
                        )
                    } else {
                        e
                    }
                })?;
                if sha256_hex(&disk_bytes) != b.file_sha256 {
                    return Err(EngineError::new(
                        Code::ExternalModification,
                        Stage::Save,
                        "project file on disk was modified outside this session",
                    ));
                }
                ModeState::Bound(b)
            }
        };

        let encoded = format::encode(doc, revision, &self.limits)?;
        let new_sha256 = sha256_hex(&encoded);
        let (tmp_name, tmp_fd) = self.write_temp_file(&a.dir, &a.name, &encoded, Stage::Save)?;

        let pre_publish = (|| -> Result<()> {
            let (reread, _) = Self::read_regular_nofollow(
                &a.dir,
                &tmp_name,
                self.limits.max_project_bytes,
                Stage::Save,
            )?;
            if reread != encoded {
                return Err(integrity("temporary project bytes changed before publish"));
            }
            let decoded = format::decode(reread, &self.limits)?;
            validate(&decoded.current)?;
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(Stage::Save, "injected temp fsync failure"));
            }
            fsync(&tmp_fd).map_err(|e| io_err(Stage::Save, format!("fsync temp project: {e}")))?;

            let dir_st =
                fstat(&a.dir).map_err(|e| io_err(Stage::Save, format!("fstat dir: {e}")))?;
            if stat_id(&dir_st) != a.parent_id {
                return Err(path_err(Stage::Save, "parent directory identity changed"));
            }
            match &mode {
                ModeState::First {
                    lock_name,
                    lock_fd,
                    lock_id,
                } => {
                    let fst = fstat(lock_fd).map_err(|_| {
                        EngineError::new(Code::ProjectLocked, Stage::Save, "lock fd invalid")
                    })?;
                    let lst = statat(&a.dir, lock_name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                        .map_err(|_| {
                            EngineError::new(Code::ProjectLocked, Stage::Save, "lockfile removed")
                        })?;
                    #[allow(clippy::useless_conversion)]
                    if !is_reg(&lst)
                        || u64::from(lst.st_nlink) != 1
                        || stat_id(&fst) != *lock_id
                        || stat_id(&lst) != *lock_id
                    {
                        return Err(EngineError::new(
                            Code::ProjectLocked,
                            Stage::Save,
                            "lockfile replaced before publication",
                        ));
                    }
                    if self.take_fault(FaultStep::Rename) {
                        return Err(io_err(Stage::Save, "injected rename failure"));
                    }
                    renameat_with(
                        &a.dir,
                        tmp_name.as_str(),
                        &a.dir,
                        a.name.as_str(),
                        RenameFlags::NOREPLACE,
                    )
                    .map_err(|e| {
                        if e == rustix::io::Errno::EXIST {
                            EngineError::new(
                                Code::TargetExists,
                                Stage::Save,
                                "project file created concurrently before publication",
                            )
                        } else {
                            io_err(Stage::Save, format!("renameat_with project: {e}"))
                        }
                    })?;
                }
                ModeState::Bound(b) => {
                    Self::verify_bound_lock(&a.dir, b, Stage::Save)?;
                    let (disk_bytes, disk_id) = Self::read_regular_nofollow(
                        &a.dir,
                        &a.name,
                        self.limits.max_project_bytes,
                        Stage::Save,
                    )
                    .map_err(|_| {
                        EngineError::new(
                            Code::ExternalModification,
                            Stage::Save,
                            "bound project file removed before rename",
                        )
                    })?;
                    self.check_not_protected(&a.canonical, Some(disk_id), Stage::Save)?;
                    if sha256_hex(&disk_bytes) != b.file_sha256 {
                        return Err(EngineError::new(
                            Code::ExternalModification,
                            Stage::Save,
                            "project file modified concurrently before rename",
                        ));
                    }
                    if self.take_fault(FaultStep::Rename) {
                        return Err(io_err(Stage::Save, "injected rename failure"));
                    }
                    renameat(&a.dir, tmp_name.as_str(), &a.dir, a.name.as_str())
                        .map_err(|e| io_err(Stage::Save, format!("renameat project: {e}")))?;
                }
            }
            Ok(())
        })();

        if let Err(e) = pre_publish {
            let _ = unlinkat(&a.dir, tmp_name.as_str(), AtFlags::empty());
            return Err(e);
        }

        // Publication succeeded; update session binding BEFORE parent fsync so
        // SAVE_DURABILITY_UNCERTAIN still tracks the newly published hash/lock.
        match mode {
            ModeState::First {
                lock_name,
                lock_fd,
                lock_id,
            } => {
                *binding = Some(ProjectBinding {
                    path: a.canonical.clone(),
                    parent_id: a.parent_id,
                    lock_name,
                    lock_id,
                    lock_fd,
                    file_sha256: new_sha256.clone(),
                    document_id: doc.document_id.clone(),
                });
            }
            ModeState::Bound(b) => {
                b.file_sha256 = new_sha256.clone();
            }
        }

        let parent_sync = if self.take_fault(FaultStep::ParentFsync) {
            Err(rustix::io::Errno::IO)
        } else {
            fsync(&a.dir)
        };
        if let Err(e) = parent_sync {
            return Err(EngineError::new(
                Code::SaveDurabilityUncertain,
                Stage::Save,
                format!("project published but parent directory fsync failed: {e}"),
            ));
        }

        Ok(SaveReceipt {
            path: a.canonical,
            sha256: new_sha256,
            revision,
        })
    }

    /// Publish caller-validated PNG/SVG bytes directly under `output_root`.
    ///
    /// The generated name is random and never replaces an existing entry. This
    /// storage API does not parse images; the caller owns image validation.
    /// Payload plus readback is bounded by `max_session_memory`. If the parent
    /// fsync fails after publication, the error detail contains the published
    /// receipt's `path`, `sha256`, and `revision`; the file is not rolled back.
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

        let id = hae_model::new_id("preview-").map_err(|e| io_err(stage, e.to_string()))?;
        let path = self.output_root.join(format!("{id}.{extension}"));
        let a = self.resolve_output(&path, stage)?;
        // resolve_output traverses the root and parent separately. For this
        // direct child, verify the actual I/O descriptor is the configured root
        // as well, closing a directory replacement between those traversals.
        if a.parent_id != self.output_root_id {
            return Err(path_err(stage, "configured output root identity changed"));
        }
        self.check_not_protected(&a.canonical, None, stage)?;
        match statat(&a.dir, a.name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(st) => {
                self.check_not_protected(&a.canonical, Some(stat_id(&st)), stage)?;
                return Err(EngineError::new(
                    Code::TargetExists,
                    stage,
                    "preview artifact target already exists",
                ));
            }
            Err(e) if e == rustix::io::Errno::NOENT => {}
            Err(e) => return Err(io_err(stage, format!("stat preview target: {e}"))),
        }

        let sha256 = sha256_hex(payload);
        let (tmp_name, tmp_fd) = self.write_temp_file(&a.dir, &a.name, payload, stage)?;
        let pre_publish = (|| -> Result<()> {
            // Explicitly set 0600 even when the process umask is more restrictive.
            rustix::fs::fchmod(&tmp_fd, Mode::from_raw_mode(0o600))
                .map_err(|e| io_err(stage, format!("chmod temp preview: {e}")))?;
            let (reread, reread_id) =
                Self::read_regular_nofollow(&a.dir, &tmp_name, max_bytes, stage)?;
            let tmp_stat =
                fstat(&tmp_fd).map_err(|e| io_err(stage, format!("fstat temp preview: {e}")))?;
            if reread_id != stat_id(&tmp_stat) || reread != payload || sha256_hex(&reread) != sha256
            {
                return Err(io_err(stage, "temporary preview identity or bytes changed"));
            }
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(stage, "injected temp fsync failure"));
            }
            fsync(&tmp_fd).map_err(|e| io_err(stage, format!("fsync temp preview: {e}")))?;

            // Refuse a root path swap before publication; all writes and cleanup
            // still use the original, identity-checked directory descriptor.
            let current = self.resolve_output(&a.canonical, stage)?;
            if current.parent_id != a.parent_id {
                return Err(path_err(stage, "preview parent directory identity changed"));
            }
            let current_tmp = statat(&a.dir, tmp_name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|e| io_err(stage, format!("stat temp preview: {e}")))?;
            if !is_reg(&current_tmp) || stat_id(&current_tmp) != reread_id {
                return Err(path_err(
                    stage,
                    "temporary preview replaced before publication",
                ));
            }
            self.check_not_protected(&a.canonical, None, stage)?;
            if self.take_fault(FaultStep::Rename) {
                return Err(io_err(stage, "injected rename failure"));
            }
            renameat_with(
                &a.dir,
                tmp_name.as_str(),
                &a.dir,
                a.name.as_str(),
                RenameFlags::NOREPLACE,
            )
            .map_err(|e| {
                if e == rustix::io::Errno::EXIST {
                    EngineError::new(
                        Code::TargetExists,
                        stage,
                        "preview artifact target created concurrently",
                    )
                } else {
                    io_err(stage, format!("renameat_with preview: {e}"))
                }
            })?;
            Ok(())
        })();
        if let Err(e) = pre_publish {
            let _ = unlinkat(&a.dir, tmp_name.as_str(), AtFlags::empty());
            return Err(e);
        }

        let receipt = SaveReceipt {
            path: a.canonical,
            sha256,
            revision,
        };
        let parent_sync = if self.take_fault(FaultStep::ParentFsync) {
            Err(rustix::io::Errno::IO)
        } else {
            fsync(&a.dir)
        };
        if let Err(e) = parent_sync {
            return Err(EngineError::new(
                Code::SaveDurabilityUncertain,
                stage,
                format!("preview artifact published but parent directory fsync failed: {e}"),
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
        let a = self.resolve_output(path.as_ref(), Stage::Export)?;
        let existing_stat = statat(&a.dir, a.name.as_str(), AtFlags::SYMLINK_NOFOLLOW).ok();
        self.check_not_protected(
            &a.canonical,
            existing_stat.as_ref().map(stat_id),
            Stage::Export,
        )?;
        if existing_stat.is_some() {
            return Err(EngineError::new(
                Code::TargetExists,
                Stage::Export,
                "export target already exists",
            ));
        }

        let bytes = format::export_bytes(doc, &self.limits)?;
        check_size(bytes.len(), self.limits.max_hwpx_bytes)?;
        let sha256 = sha256_hex(&bytes);
        let (tmp_name, tmp_fd) = self.write_temp_file(&a.dir, &a.name, &bytes, Stage::Export)?;

        let pre_publish = (|| -> Result<()> {
            let (reread, _) = Self::read_regular_nofollow(
                &a.dir,
                &tmp_name,
                self.limits.max_hwpx_bytes,
                Stage::Export,
            )?;
            if reread != bytes {
                return Err(io_err(Stage::Export, "temporary export bytes changed"));
            }
            let archive = Archive::open(reread.clone(), &self.limits)?;
            let state = PackageState::from_archive(&archive);
            hae_package::analyze(
                &state,
                archive.entries().first().is_some_and(|e| e.is_stored()),
                &self.limits,
            )?;
            validate(&reread)?;
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(Stage::Export, "injected temp fsync failure"));
            }
            fsync(&tmp_fd).map_err(|e| io_err(Stage::Export, format!("fsync temp export: {e}")))?;
            if self.take_fault(FaultStep::Rename) {
                return Err(io_err(Stage::Export, "injected rename failure"));
            }
            renameat_with(
                &a.dir,
                tmp_name.as_str(),
                &a.dir,
                a.name.as_str(),
                RenameFlags::NOREPLACE,
            )
            .map_err(|e| {
                if e == rustix::io::Errno::EXIST {
                    EngineError::new(
                        Code::TargetExists,
                        Stage::Export,
                        "export target created concurrently",
                    )
                } else {
                    io_err(Stage::Export, format!("renameat_with export: {e}"))
                }
            })?;
            Ok(())
        })();

        if let Err(e) = pre_publish {
            let _ = unlinkat(&a.dir, tmp_name.as_str(), AtFlags::empty());
            return Err(e);
        }

        let parent_sync = if self.take_fault(FaultStep::ParentFsync) {
            Err(rustix::io::Errno::IO)
        } else {
            fsync(&a.dir)
        };
        if let Err(e) = parent_sync {
            return Err(EngineError::new(
                Code::SaveDurabilityUncertain,
                Stage::Export,
                format!("export published but parent directory fsync failed: {e}"),
            ));
        }

        Ok(SaveReceipt {
            path: a.canonical,
            sha256,
            revision: 0,
        })
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
        let a = self.resolve_output(path.as_ref(), stage)?;
        let existing_stat = statat(&a.dir, a.name.as_str(), AtFlags::SYMLINK_NOFOLLOW).ok();
        self.check_not_protected(&a.canonical, existing_stat.as_ref().map(stat_id), stage)?;
        if existing_stat.is_some() {
            return Err(EngineError::new(
                Code::TargetExists,
                stage,
                "export target already exists",
            ));
        }

        let sha256 = sha256_hex(bytes);
        let (tmp_name, tmp_fd) = self.write_temp_file(&a.dir, &a.name, bytes, stage)?;
        let pre_publish = (|| -> Result<()> {
            let (reread, _) = Self::read_regular_nofollow(&a.dir, &tmp_name, max, stage)?;
            if reread != bytes {
                return Err(io_err(stage, "temporary text export bytes changed"));
            }
            if self.take_fault(FaultStep::TempFsync) {
                return Err(io_err(stage, "injected temp fsync failure"));
            }
            fsync(&tmp_fd).map_err(|e| io_err(stage, format!("fsync temp export: {e}")))?;
            if self.take_fault(FaultStep::Rename) {
                return Err(io_err(stage, "injected rename failure"));
            }
            renameat_with(
                &a.dir,
                tmp_name.as_str(),
                &a.dir,
                a.name.as_str(),
                RenameFlags::NOREPLACE,
            )
            .map_err(|e| {
                if e == rustix::io::Errno::EXIST {
                    EngineError::new(
                        Code::TargetExists,
                        stage,
                        "export target created concurrently",
                    )
                } else {
                    io_err(stage, format!("renameat_with export: {e}"))
                }
            })?;
            Ok(())
        })();
        if let Err(e) = pre_publish {
            let _ = unlinkat(&a.dir, tmp_name.as_str(), AtFlags::empty());
            return Err(e);
        }

        let receipt = SaveReceipt {
            path: a.canonical,
            sha256,
            revision,
        };
        let parent_sync = if self.take_fault(FaultStep::ParentFsync) {
            Err(rustix::io::Errno::IO)
        } else {
            fsync(&a.dir)
        };
        if let Err(e) = parent_sync {
            return Err(EngineError::new(
                Code::SaveDurabilityUncertain,
                stage,
                format!("text export published but parent directory fsync failed: {e}"),
            )
            .with_detail(serde_json::json!({
                "path": receipt.path,
                "sha256": receipt.sha256,
            })));
        }
        Ok(receipt)
    }
}
