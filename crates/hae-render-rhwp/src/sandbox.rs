//! Product sandbox for the rhwp child process (spec §9, stage 0 S3).
//!
//! Every renderer command runs inside an unprivileged bubblewrap sandbox:
//! all namespaces unshared (including network), `--die-with-parent`,
//! `--new-session`, cleared environment, read-only `/usr`, read-only input
//! directory, read-only fonts directory and a writable `/out` only. There is
//! no code path that starts the renderer without bwrap. [`Sandbox::self_test`]
//! proves the isolation before the first use.
//!
//! Process control is argument arrays only (no shell strings in product
//! paths), a wall-clock timeout with kill, stdout/stderr caps, an output
//! directory size cap and cooperative cancellation.

use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Cooperative cancellation flag shared with the caller.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Limits for one sandboxed process.
#[derive(Debug, Clone, Copy)]
pub struct RunLimits {
    pub timeout: Duration,
    pub stdout_cap: usize,
    pub stderr_cap: usize,
    /// Total bytes allowed in `/out` (host side), checked while running.
    pub out_cap_bytes: u64,
    /// Total entries allowed in `/out`.
    pub out_cap_entries: usize,
}

impl RunLimits {
    pub fn with_timeout_secs(secs: u64) -> Self {
        RunLimits {
            timeout: Duration::from_secs(secs),
            stdout_cap: 64 * 1024 * 1024,
            stderr_cap: 8 * 1024 * 1024,
            out_cap_bytes: 512 * 1024 * 1024,
            out_cap_entries: 10_000,
        }
    }
}

/// Captured output of a finished sandboxed process.
#[derive(Debug, Clone)]
pub struct ProcOutput {
    /// Exit code; `None` if the process was terminated by a signal.
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
}

impl ProcOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Why a sandboxed run did not complete normally. The child is always dead
/// when one of these is returned.
#[derive(Debug)]
pub enum RunError {
    Spawn(std::io::Error),
    Timeout(Duration),
    Cancelled,
    StdoutLimit(usize),
    StderrLimit(usize),
    OutputLimit(String),
    Wait(std::io::Error),
}

impl RunError {
    /// Stable machine reason used in `RENDERER_UNAVAILABLE`.
    pub fn reason(&self) -> &'static str {
        match self {
            RunError::Spawn(_) => "spawn_failed",
            RunError::Timeout(_) => "timeout",
            RunError::Cancelled => "cancelled",
            RunError::StdoutLimit(_) | RunError::StderrLimit(_) | RunError::OutputLimit(_) => {
                "output_limit"
            }
            RunError::Wait(_) => "wait_failed",
        }
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Spawn(e) => write!(f, "cannot start sandbox: {e}"),
            RunError::Timeout(d) => write!(f, "renderer exceeded {} s and was killed", d.as_secs()),
            RunError::Cancelled => write!(f, "renderer run cancelled and killed"),
            RunError::StdoutLimit(n) => write!(f, "renderer stdout exceeded {n} bytes"),
            RunError::StderrLimit(n) => write!(f, "renderer stderr exceeded {n} bytes"),
            RunError::OutputLimit(m) => write!(f, "renderer output directory limit: {m}"),
            RunError::Wait(e) => write!(f, "waiting for sandbox failed: {e}"),
        }
    }
}

/// A directory removed on drop. Created with mode 0700.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new_in(parent: &Path, prefix: &str) -> std::io::Result<TempDir> {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).map_err(|e| std::io::Error::other(e.to_string()))?;
        let suffix: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let path = parent.join(format!("{prefix}{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(TempDir { path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Sandbox configuration. Paths are explicit: bwrap is never searched in PATH.
#[derive(Debug, Clone)]
pub struct Sandbox {
    bwrap: PathBuf,
    font_dir: PathBuf,
}

impl Sandbox {
    pub fn new(bwrap: impl Into<PathBuf>, font_dir: impl Into<PathBuf>) -> Self {
        Sandbox {
            bwrap: bwrap.into(),
            font_dir: font_dir.into(),
        }
    }

    pub fn bwrap_path(&self) -> &Path {
        &self.bwrap
    }

    /// The full bwrap argument array for `argv` run inside the sandbox.
    /// `/in` is read-only, `/out` is the only writable host-backed path.
    pub fn build_args(&self, in_dir: &Path, out_dir: &Path, argv: &[&str]) -> Vec<OsString> {
        let mut a: Vec<OsString> = Vec::new();
        let mut push = |s: &str| a.push(OsString::from(s));
        for s in [
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--cap-drop",
            "ALL",
            "--setenv",
            "PATH",
            "/usr/bin",
            "--setenv",
            "HOME",
            "/nonexistent",
            "--setenv",
            "LANG",
            "C.UTF-8",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib64",
            "/lib64",
            "--symlink",
            "usr/bin",
            "/sbin",
            "--ro-bind-try",
            "/etc/fonts",
            "/etc/fonts",
            "--ro-bind-try",
            "/etc/ld.so.cache",
            "/etc/ld.so.cache",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--ro-bind",
        ] {
            push(s);
        }
        a.push(in_dir.into());
        a.push("/in".into());
        a.push("--bind".into());
        a.push(out_dir.into());
        a.push("/out".into());
        a.push("--ro-bind".into());
        a.push((&self.font_dir).into());
        a.push("/fonts".into());
        a.push("--chdir".into());
        a.push("/tmp".into());
        a.push("--".into());
        for s in argv {
            a.push((*s).into());
        }
        a
    }

    /// Run `argv` inside the sandbox with the given limits.
    pub fn run(
        &self,
        in_dir: &Path,
        out_dir: &Path,
        argv: &[&str],
        limits: &RunLimits,
        cancel: Option<&CancelToken>,
    ) -> Result<ProcOutput, RunError> {
        let args = self.build_args(in_dir, out_dir, argv);
        let mut cmd = Command::new(&self.bwrap);
        cmd.args(args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        run_limited(cmd, out_dir, limits, cancel)
    }

    /// Prove isolation before first use. `Ok(())` only if every probe behaves
    /// as required; otherwise the reason names the failed probe. The caller
    /// maps any failure to `RENDERER_UNAVAILABLE(sandbox_unverified)`.
    pub fn self_test(&self, work_root: &Path) -> Result<(), String> {
        if !self.bwrap.is_file() {
            return Err(format!("bwrap not found at {}", self.bwrap.display()));
        }
        let td =
            TempDir::new_in(work_root, "hae-selftest-").map_err(|e| format!("tempdir: {e}"))?;
        let in_dir = td.path().join("in");
        let out_dir = td.path().join("out");
        for d in [&in_dir, &out_dir] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(d)
                .map_err(|e| format!("mkdir: {e}"))?;
        }
        fs::write(in_dir.join("marker"), b"ro").map_err(|e| format!("write marker: {e}"))?;
        let limits = RunLimits {
            timeout: Duration::from_secs(20),
            stdout_cap: 64 * 1024,
            stderr_cap: 64 * 1024,
            out_cap_bytes: 1024 * 1024,
            out_cap_entries: 100,
        };
        let run = |argv: &[&str]| {
            self.run(&in_dir, &out_dir, argv, &limits, None)
                .map_err(|e| format!("probe {argv:?}: {e}"))
        };

        // 1. The sandbox starts and /out is writable and host-backed.
        let w = run(&["/usr/bin/sh", "-c", "echo isolated > /out/probe"])?;
        if !w.success() {
            return Err(format!(
                "cannot write /out: {}",
                String::from_utf8_lossy(&w.stderr)
            ));
        }
        match fs::read(out_dir.join("probe")) {
            Ok(b) if b == b"isolated\n" => {}
            _ => return Err("/out is not host-backed".into()),
        }
        // 2. The input directory is read-only, for new files and existing ones.
        for cmd in ["echo x > /in/new", "echo x > /in/marker", "rm /in/marker"] {
            let r = run(&["/usr/bin/sh", "-c", cmd])?;
            if r.success() {
                return Err(format!("input directory is writable ({cmd})"));
            }
        }
        if in_dir.join("new").exists()
            || fs::read(in_dir.join("marker")).ok().as_deref() != Some(b"ro")
        {
            return Err("input directory changed on the host".into());
        }
        // 3. The system tree is read-only and the host home is not visible.
        let r = run(&["/usr/bin/sh", "-c", "echo x > /usr/probe"])?;
        if r.success() {
            return Err("/usr is writable".into());
        }
        let r = run(&["/usr/bin/sh", "-c", "test ! -e /home && test ! -e /root"])?;
        if !r.success() {
            return Err("host home directories are visible".into());
        }
        // 4. Writes outside /out never reach the host.
        let r = run(&["/usr/bin/sh", "-c", "echo x > /tmp/outside"])?;
        if !r.success() || out_dir.join("outside").exists() {
            return Err("temporary root is not private".into());
        }
        // 5. No network: only the loopback device exists in the new namespace.
        let r = run(&["/usr/bin/cat", "/proc/net/dev"])?;
        if !r.success() {
            return Err("cannot read /proc/net/dev".into());
        }
        let text = String::from_utf8_lossy(&r.stdout);
        let ifaces: Vec<&str> = text
            .lines()
            .skip(2)
            .filter_map(|l| l.split(':').next())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .collect();
        if ifaces.iter().any(|n| *n != "lo") {
            return Err(format!("network interfaces visible in sandbox: {ifaces:?}"));
        }
        // 6. The environment is cleared.
        let r = run(&["/usr/bin/env"])?;
        let env = String::from_utf8_lossy(&r.stdout);
        let leaked: Vec<&str> = env
            .lines()
            .filter_map(|l| l.split('=').next())
            .filter(|k| {
                !matches!(
                    *k,
                    "PATH" | "HOME" | "LANG" | "PWD" | "SHLVL" | "_" | "OLDPWD"
                )
            })
            .collect();
        if !leaked.is_empty() {
            return Err(format!("environment variables leaked: {leaked:?}"));
        }
        Ok(())
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut r: R,
    cap: usize,
    overflow: Arc<AtomicBool>,
) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.len() + n > cap {
                        overflow.store(true, Ordering::SeqCst);
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
            }
        }
        let _ = tx.send(buf);
    });
    rx
}

/// Total size and entry count under `dir`, not following symlinks. Errors
/// with a message as soon as a cap is exceeded.
pub fn dir_usage(dir: &Path, cap_bytes: u64, cap_entries: usize) -> Result<(u64, usize), String> {
    let mut bytes = 0u64;
    let mut entries = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = match fs::read_dir(&d) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            entries += 1;
            if entries > cap_entries {
                return Err(format!("more than {cap_entries} entries"));
            }
            let Ok(md) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            if md.is_dir() {
                stack.push(e.path());
            } else {
                bytes = bytes.saturating_add(md.len());
                if bytes > cap_bytes {
                    return Err(format!("more than {cap_bytes} bytes"));
                }
            }
        }
    }
    Ok((bytes, entries))
}

/// Spawn `cmd` (already configured with piped stdout/stderr) and supervise it.
pub fn run_limited(
    mut cmd: Command,
    out_dir: &Path,
    limits: &RunLimits,
    cancel: Option<&CancelToken>,
) -> Result<ProcOutput, RunError> {
    let started = Instant::now();
    let mut child = cmd.spawn().map_err(RunError::Spawn)?;
    let out_over = Arc::new(AtomicBool::new(false));
    let err_over = Arc::new(AtomicBool::new(false));
    let out_rx = spawn_reader(
        child.stdout.take().expect("piped stdout"),
        limits.stdout_cap,
        out_over.clone(),
    );
    let err_rx = spawn_reader(
        child.stderr.take().expect("piped stderr"),
        limits.stderr_cap,
        err_over.clone(),
    );

    let mut last_scan = Instant::now();
    let failure: Option<RunError> = loop {
        match child.try_wait() {
            Ok(Some(_)) => break None,
            Ok(None) => {}
            Err(e) => break Some(RunError::Wait(e)),
        }
        if cancel.is_some_and(CancelToken::is_cancelled) {
            break Some(RunError::Cancelled);
        }
        if started.elapsed() >= limits.timeout {
            break Some(RunError::Timeout(limits.timeout));
        }
        if out_over.load(Ordering::SeqCst) {
            break Some(RunError::StdoutLimit(limits.stdout_cap));
        }
        if err_over.load(Ordering::SeqCst) {
            break Some(RunError::StderrLimit(limits.stderr_cap));
        }
        if last_scan.elapsed() >= Duration::from_millis(100) {
            last_scan = Instant::now();
            if let Err(m) = dir_usage(out_dir, limits.out_cap_bytes, limits.out_cap_entries) {
                break Some(RunError::OutputLimit(m));
            }
        }
        thread::sleep(Duration::from_millis(5));
    };

    if let Some(err) = failure {
        // SIGKILL to bwrap; its pid namespace init dies with it (--die-with-parent).
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }
    let status = child.wait().map_err(RunError::Wait)?;
    let grace = Duration::from_secs(2);
    let stdout = out_rx.recv_timeout(grace).unwrap_or_default();
    let stderr = err_rx.recv_timeout(grace).unwrap_or_default();
    if out_over.load(Ordering::SeqCst) {
        return Err(RunError::StdoutLimit(limits.stdout_cap));
    }
    if err_over.load(Ordering::SeqCst) {
        return Err(RunError::StderrLimit(limits.stderr_cap));
    }
    if let Err(m) = dir_usage(out_dir, limits.out_cap_bytes, limits.out_cap_entries) {
        return Err(RunError::OutputLimit(m));
    }
    Ok(ProcOutput {
        code: status.code(),
        stdout,
        stderr,
        elapsed: started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> Sandbox {
        Sandbox::new("/usr/bin/bwrap", "/usr/share/fonts")
    }

    struct Dirs {
        _td: TempDir,
        in_dir: PathBuf,
        out_dir: PathBuf,
    }

    fn dirs() -> Dirs {
        let td = TempDir::new_in(&std::env::temp_dir(), "hae-sbx-test-").unwrap();
        let in_dir = td.path().join("in");
        let out_dir = td.path().join("out");
        fs::create_dir(&in_dir).unwrap();
        fs::create_dir(&out_dir).unwrap();
        Dirs {
            _td: td,
            in_dir,
            out_dir,
        }
    }

    #[test]
    fn self_test_proves_isolation() {
        sandbox()
            .self_test(&std::env::temp_dir())
            .expect("isolation self-test");
    }

    #[test]
    fn self_test_fails_when_bwrap_is_missing() {
        let sb = Sandbox::new("/nonexistent/bwrap", "/usr/share/fonts");
        let e = sb.self_test(&std::env::temp_dir()).unwrap_err();
        assert!(e.contains("bwrap not found"), "{e}");
    }

    #[test]
    fn args_are_an_array_with_fixed_isolation_flags() {
        let d = dirs();
        let a = sandbox().build_args(&d.in_dir, &d.out_dir, &["/in/rhwp", "--version"]);
        let s: Vec<String> = a.iter().map(|x| x.to_string_lossy().into_owned()).collect();
        for flag in [
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
        ] {
            assert!(s.contains(&flag.to_string()), "missing {flag}");
        }
        let dash = s.iter().position(|x| x == "--").unwrap();
        assert_eq!(&s[dash + 1..], ["/in/rhwp", "--version"]);
        // /in is bound read-only, /out is the only plain bind.
        let ro = s
            .iter()
            .position(|x| *x == d.in_dir.to_string_lossy())
            .unwrap();
        assert_eq!(s[ro - 1], "--ro-bind");
        assert_eq!(s.iter().filter(|x| *x == "--bind").count(), 1);
    }

    #[test]
    fn timeout_kills_the_child() {
        let d = dirs();
        let mut l = RunLimits::with_timeout_secs(1);
        l.timeout = Duration::from_millis(700);
        let t = Instant::now();
        let e = sandbox()
            .run(&d.in_dir, &d.out_dir, &["/usr/bin/sleep", "30"], &l, None)
            .unwrap_err();
        assert!(matches!(e, RunError::Timeout(_)), "{e}");
        assert!(t.elapsed() < Duration::from_secs(10));
        assert_eq!(e.reason(), "timeout");
    }

    #[test]
    fn cancellation_kills_the_child() {
        let d = dirs();
        let token = CancelToken::new();
        let t2 = token.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            t2.cancel();
        });
        let l = RunLimits::with_timeout_secs(30);
        let t = Instant::now();
        let e = sandbox()
            .run(
                &d.in_dir,
                &d.out_dir,
                &["/usr/bin/sleep", "30"],
                &l,
                Some(&token),
            )
            .unwrap_err();
        assert!(matches!(e, RunError::Cancelled), "{e}");
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn stdout_cap_stops_a_flood() {
        let d = dirs();
        let mut l = RunLimits::with_timeout_secs(30);
        l.stdout_cap = 100_000;
        let e = sandbox()
            .run(&d.in_dir, &d.out_dir, &["/usr/bin/yes"], &l, None)
            .unwrap_err();
        assert!(matches!(e, RunError::StdoutLimit(_)), "{e}");
    }

    #[test]
    fn stderr_cap_stops_a_flood() {
        let d = dirs();
        let mut l = RunLimits::with_timeout_secs(30);
        l.stderr_cap = 100_000;
        let e = sandbox()
            .run(
                &d.in_dir,
                &d.out_dir,
                &["/usr/bin/sh", "-c", "yes 1>&2"],
                &l,
                None,
            )
            .unwrap_err();
        assert!(matches!(e, RunError::StderrLimit(_)), "{e}");
    }

    #[test]
    fn output_dir_cap_stops_a_disk_filler() {
        let d = dirs();
        let mut l = RunLimits::with_timeout_secs(30);
        l.out_cap_bytes = 4 * 1024 * 1024;
        let e = sandbox()
            .run(
                &d.in_dir,
                &d.out_dir,
                &[
                    "/usr/bin/sh",
                    "-c",
                    "while true; do head -c 1000000 /dev/zero >> /out/big; done",
                ],
                &l,
                None,
            )
            .unwrap_err();
        assert!(matches!(e, RunError::OutputLimit(_)), "{e}");
    }

    #[test]
    fn nonzero_exit_is_reported_not_hidden() {
        let d = dirs();
        let l = RunLimits::with_timeout_secs(30);
        let o = sandbox()
            .run(&d.in_dir, &d.out_dir, &["/usr/bin/false"], &l, None)
            .unwrap();
        assert_eq!(o.code, Some(1));
        assert!(!o.success());
    }
}
