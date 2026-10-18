//! Windows preview path: explicit opt-in without verified isolation.
//!
//! Linux runs the renderer inside bubblewrap with `prlimit --as`; the adapter
//! proves that isolation with [`crate::sandbox::Sandbox::self_test`] before
//! the first run. Windows has no bubblewrap, so the default behaviour there is
//! unchanged: `RENDERER_UNAVAILABLE(sandbox_unverified)` and the renderer is
//! never started.
//!
//! This module adds a second, explicit path. It runs the renderer without any
//! equivalent isolation, and only when `HAE_ALLOW_UNSANDBOXED_PREVIEW` is set
//! to an affirmative value. Everything it can still enforce is applied (wall
//! clock timeout with kill, stdout/stderr caps, output-directory caps,
//! cancellation, a cleared environment, a private temporary directory, the
//! same staged input copy and the same `/out`-only output reading); everything
//! it cannot enforce is listed in the result metadata and as a
//! `SANDBOX_UNVERIFIED` warning, so a preview can never look like a verified
//! sandboxed run.
//!
//! What this build does *not* apply is a Windows Job Object. The workspace
//! forbids `unsafe` (`unsafe_code = "forbid"`), and creating or assigning a Job
//! Object needs FFI to `CreateJobObjectW`/`SetInformationJobObject`/
//! `AssignProcessToJobObject`. [`job_object_limits`] therefore computes the
//! declared limits for that missing backend (one source of truth, so the
//! decision is testable) and every result records `applied: false`.

use serde::Serialize;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Opt-in switch for running the renderer without verified isolation.
pub const ALLOW_ENV: &str = "HAE_ALLOW_UNSANDBOXED_PREVIEW";

/// Reason attached to every unsandboxed result.
pub const UNVERIFIED_REASON: &str = "sandbox_unverified";

/// Largest number of processes the Job Object limits below are computed for.
pub const MAX_UNSANDBOXED_PROCESSES: u32 = 8;

/// Whether the caller explicitly allowed an unsandboxed preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    /// Refuse (the default on every platform).
    #[default]
    Off,
    /// Run without verified isolation; results carry `sandbox_unverified`.
    On,
}

impl Policy {
    /// Affirmative values only. Anything else — including unusual spellings —
    /// leaves the safe default in place.
    pub fn from_value(value: Option<&OsStr>) -> Self {
        match value.and_then(OsStr::to_str).map(str::trim) {
            Some(v) if v.eq_ignore_ascii_case("1") => Policy::On,
            Some(v) if v.eq_ignore_ascii_case("true") => Policy::On,
            Some(v) if v.eq_ignore_ascii_case("yes") => Policy::On,
            Some(v) if v.eq_ignore_ascii_case("on") => Policy::On,
            _ => Policy::Off,
        }
    }

    pub fn from_process() -> Self {
        Policy::from_value(std::env::var_os(ALLOW_ENV).as_deref())
    }

    pub fn is_on(self) -> bool {
        matches!(self, Policy::On)
    }
}

/// Limits a Windows Job Object backend must apply. Declared, not applied: see
/// the module comment. Values are derived from the same budget the Linux path
/// passes to `prlimit --as` so a future backend cannot silently use a different
/// number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobObjectLimits {
    /// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    pub kill_on_job_close: bool,
    /// `JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION`.
    pub die_on_unhandled_exception: bool,
    /// `JOB_OBJECT_LIMIT_ACTIVE_PROCESS`.
    pub active_process_limit: u32,
    /// `JOB_OBJECT_LIMIT_PROCESS_MEMORY` (per process, committed bytes).
    pub process_memory_bytes: u64,
    /// `JOB_OBJECT_LIMIT_JOB_MEMORY` (aggregate committed bytes).
    pub job_memory_bytes: u64,
    /// `JOB_OBJECT_LIMIT_PROCESS_TIME` (per process, milliseconds).
    pub process_cpu_time_ms: u64,
    /// `JOB_OBJECT_LIMIT_JOB_TIME` (whole job, milliseconds).
    pub job_cpu_time_ms: u64,
}

/// Declared Job Object limits for one renderer run.
pub fn job_object_limits(address_space_bytes: u64, timeout: Duration) -> JobObjectLimits {
    let ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    JobObjectLimits {
        kill_on_job_close: true,
        die_on_unhandled_exception: true,
        active_process_limit: MAX_UNSANDBOXED_PROCESSES,
        process_memory_bytes: address_space_bytes,
        job_memory_bytes: address_space_bytes,
        process_cpu_time_ms: ms,
        job_cpu_time_ms: ms.saturating_mul(u64::from(MAX_UNSANDBOXED_PROCESSES)),
    }
}

/// How one guarantee is covered on the unsandboxed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enforcement {
    pub item: &'static str,
    pub enforced: bool,
    pub mechanism: &'static str,
    /// What is *not* guaranteed, in the same words the docs use.
    pub limit: &'static str,
}

/// The enforced/unenforced split of the unsandboxed path. Data rather than
/// prose so the result metadata, `doctor` and the platform table agree.
pub fn enforcement() -> Vec<Enforcement> {
    vec![
        Enforcement {
            item: "wallClockTimeout",
            enforced: true,
            mechanism: "std process wait + kill (taskkill /T then kill)",
            limit: "kills the tree within the timeout; a process that ignores termination may survive briefly",
        },
        Enforcement {
            item: "stdoutStderrCaps",
            enforced: true,
            mechanism: "parent-side reader threads with byte caps",
            limit: "a cap breach aborts the run with output_limit",
        },
        Enforcement {
            item: "outputDirectoryCaps",
            enforced: true,
            mechanism: "bounded directory scan while running (bytes and entries)",
            limit: "sampled, not a filesystem quota",
        },
        Enforcement {
            item: "cancellation",
            enforced: true,
            mechanism: "CancelToken checked by the supervising loop",
            limit: "cooperative: only the supervised process tree is killed",
        },
        Enforcement {
            item: "environment",
            enforced: true,
            mechanism: "env_clear plus a fixed minimal set (SystemRoot, PATH, TEMP/TMP, windir)",
            limit: "a weaker proof than Linux --clearenv; the child still sees its own OS environment",
        },
        Enforcement {
            item: "stagedInput",
            enforced: true,
            mechanism: "renderer and document snapshot copied into a private temporary directory; \
                        the child's working directory, TEMP and TMP point there",
            limit: "a copy, not a read-only mount: nothing stops the process from opening other host paths",
        },
        Enforcement {
            item: "filesystemConfinement",
            enforced: false,
            mechanism: "none",
            limit: "no namespace or ACL confinement: the renderer can read and write host paths the user account can",
        },
        Enforcement {
            item: "readOnlyInput",
            enforced: false,
            mechanism: "none",
            limit: "the input directory is a private copy, but it is writable by the running process",
        },
        Enforcement {
            item: "memoryLimit",
            enforced: false,
            mechanism: "none (Job Object backend absent in this build)",
            limit: "no per-process or aggregate memory cap; RLIMIT_AS equivalent is not applied",
        },
        Enforcement {
            item: "processCountLimit",
            enforced: false,
            mechanism: "none (Job Object backend absent in this build)",
            limit: "the renderer may spawn unlimited child processes",
        },
        Enforcement {
            item: "cpuTimeLimit",
            enforced: false,
            mechanism: "none (Job Object backend absent in this build)",
            limit: "only the wall-clock timeout bounds CPU use",
        },
        Enforcement {
            item: "networkIsolation",
            enforced: false,
            mechanism: "none",
            limit: "the renderer keeps the machine's network access",
        },
    ]
}

/// The refusal message used when isolation is unavailable and the opt-in is
/// off. Always says how to opt in and what opting in costs.
pub fn refusal_message() -> String {
    format!(
        "{} Set {ALLOW_ENV}=1 to run it anyway without isolation; every such preview is marked \
         sandbox_unverified and is not equivalent to Linux bubblewrap isolation.",
        crate::sandbox::unsupported_platform_message()
    )
}

/// `metadata.sandbox` for a run without verified isolation. It never claims a
/// self-test, lists what is enforced and what is not, and records the Job
/// Object limits a missing backend would have to apply.
pub fn describe(address_space_bytes: u64, timeout: Duration) -> serde_json::Value {
    serde_json::json!({
        "tool": "none",
        "version": serde_json::Value::Null,
        "selfTest": "not_applicable",
        "verified": false,
        "reason": UNVERIFIED_REASON,
        "platform": std::env::consts::OS,
        "optIn": ALLOW_ENV,
        "equivalentToLinuxBubblewrap": false,
        "enforcement": enforcement(),
        "jobObject": {
            "applied": false,
            "reason": "this build has no Windows Job Object backend: the workspace forbids unsafe FFI \
                       (CreateJobObjectW/SetInformationJobObject/AssignProcessToJobObject)",
            "declared": job_object_limits(address_space_bytes, timeout),
        },
        "memoryLimit": {
            "tool": serde_json::Value::Null,
            "resource": "process and job committed memory",
            "applied": false,
            "reason": "no Job Object backend in this build; no RLIMIT_AS equivalent is applied",
        },
        "rasterMemoryLimit": serde_json::Value::Null,
        "timeoutSecs": timeout.as_secs(),
    })
}

/// Translate the adapter's in-sandbox argv (`/in/rhwp`, `/in/doc.hwpx`,
/// `/out`, `/tmp`) into real host paths. Without a mount namespace those
/// prefixes are not mount points, and an unsandboxed run must not be able to
/// name a host path by accident, so any other absolute argument is rejected.
pub fn translate_argv(
    argv: &[&str],
    in_dir: &Path,
    out_dir: &Path,
    tmp_dir: &Path,
    fonts_dir: Option<&Path>,
) -> std::result::Result<Vec<OsString>, String> {
    let remap = |arg: &str| -> std::result::Result<OsString, String> {
        let (base, rest) = if let Some(rest) = strip_prefix(arg, "/in") {
            (in_dir, rest)
        } else if let Some(rest) = strip_prefix(arg, "/out") {
            (out_dir, rest)
        } else if let Some(rest) = strip_prefix(arg, "/tmp") {
            (tmp_dir, rest)
        } else if let Some(rest) = strip_prefix(arg, "/fonts") {
            (
                fonts_dir
                    .ok_or_else(|| "/fonts is not available to an unsandboxed run".to_string())?,
                rest,
            )
        } else if arg.starts_with('/') {
            return Err(format!(
                "unsandboxed run may not reference the host path {arg:?}"
            ));
        } else {
            return Ok(OsString::from(arg));
        };
        let mut p = PathBuf::from(base);
        for part in rest.split('/').filter(|s| !s.is_empty()) {
            p.push(part);
        }
        Ok(p.into_os_string())
    };
    argv.iter().map(|a| remap(a)).collect()
}

/// `/in`, `/input` → `Some("/input")`; `/index` → `None`.
fn strip_prefix<'a>(arg: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = arg.strip_prefix(prefix)?;
    if rest.is_empty() || rest.starts_with('/') {
        Some(rest)
    } else {
        None
    }
}

/// Windows runner for the opt-in path. It reuses the shared supervision loop
/// in [`crate::sandbox::run_limited`]; only the command construction differs.
#[cfg(windows)]
pub struct Runner;

#[cfg(windows)]
impl Default for Runner {
    fn default() -> Self {
        Runner
    }
}

#[cfg(windows)]
impl Runner {
    pub fn new() -> Self {
        Runner
    }

    /// Run `argv` with every limit this platform can enforce. `memory_bytes` is
    /// accepted for symmetry with the Linux path but is *not* applied (no Job
    /// Object backend); the result metadata records that.
    pub fn run_with_memory(
        &self,
        in_dir: &Path,
        out_dir: &Path,
        argv: &[&str],
        limits: &crate::sandbox::RunLimits,
        cancel: Option<&crate::sandbox::CancelToken>,
        _memory_bytes: Option<u64>,
    ) -> std::result::Result<crate::sandbox::ProcOutput, crate::sandbox::RunError> {
        use crate::sandbox::{RunError, TempDir};
        let tmp =
            TempDir::new_in(&std::env::temp_dir(), "hae-unsandboxed-").map_err(RunError::Spawn)?;
        let args = translate_argv(argv, in_dir, out_dir, tmp.path(), None)
            .map_err(|m| RunError::Spawn(std::io::Error::other(m)))?;
        let (program, rest) = args
            .split_first()
            .ok_or_else(|| RunError::Spawn(std::io::Error::other("empty renderer argv")))?;
        let mut cmd = std::process::Command::new(program);
        cmd.args(rest).env_clear();
        // The minimum a Windows process needs to start; everything else is
        // dropped. Documented as a weaker proof than Linux --clearenv.
        for key in ["SystemRoot", "windir"] {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        cmd.env("TEMP", tmp.path())
            .env("TMP", tmp.path())
            .env("PATH", in_dir)
            // The child gets a private working directory, not the caller's.
            .current_dir(tmp.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        crate::sandbox::run_limited(cmd, out_dir, limits, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::TempDir;
    use std::fs;

    #[test]
    fn policy_is_on_only_for_affirmative_values() {
        for on in ["1", "true", "TRUE", " yes ", "On"] {
            assert_eq!(
                Policy::from_value(Some(OsStr::new(on))),
                Policy::On,
                "{on} should opt in"
            );
        }
        for off in ["", "0", "false", "off", "no", "yes please", "unsandboxed"] {
            assert_eq!(
                Policy::from_value(Some(OsStr::new(off))),
                Policy::Off,
                "{off} must not opt in"
            );
        }
        assert_eq!(Policy::from_value(None), Policy::Off);
        assert!(!Policy::default().is_on(), "the default refuses");
    }

    #[test]
    fn job_object_limits_follow_the_linux_budget_and_the_timeout() {
        let l = job_object_limits(512 * 1024 * 1024, Duration::from_secs(60));
        assert_eq!(l.process_memory_bytes, 512 * 1024 * 1024);
        assert_eq!(l.job_memory_bytes, 512 * 1024 * 1024);
        assert_eq!(l.active_process_limit, MAX_UNSANDBOXED_PROCESSES);
        assert_eq!(l.process_cpu_time_ms, 60_000);
        assert_eq!(
            l.job_cpu_time_ms,
            60_000 * u64::from(MAX_UNSANDBOXED_PROCESSES)
        );
        assert!(l.kill_on_job_close && l.die_on_unhandled_exception);
        // The limits are declared, never claimed as applied.
        assert!(enforcement()
            .iter()
            .any(|e| e.item == "memoryLimit" && !e.enforced));
    }

    #[test]
    fn describe_never_claims_a_verified_sandbox() {
        let d = describe(512 * 1024 * 1024, Duration::from_secs(60));
        assert_eq!(d["tool"], "none");
        assert_eq!(d["selfTest"], "not_applicable");
        assert_eq!(d["verified"], false);
        assert_eq!(d["equivalentToLinuxBubblewrap"], false);
        assert_eq!(d["reason"], UNVERIFIED_REASON);
        assert_eq!(d["optIn"], ALLOW_ENV);
        assert_eq!(d["jobObject"]["applied"], false);
        assert_eq!(
            d["jobObject"]["declared"]["processMemoryBytes"],
            512 * 1024 * 1024
        );
        assert_eq!(d["memoryLimit"]["applied"], false);
        let enforced: Vec<&str> = d["enforcement"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["enforced"] == true)
            .map(|e| e["item"].as_str().unwrap())
            .collect();
        assert!(enforced.contains(&"wallClockTimeout"));
        assert!(
            !enforced.contains(&"filesystemConfinement"),
            "the unsandboxed path must not claim filesystem confinement"
        );
    }

    #[test]
    fn every_enforcement_item_states_what_it_cannot_do() {
        let all = enforcement();
        for key in [
            "wallClockTimeout",
            "filesystemConfinement",
            "memoryLimit",
            "processCountLimit",
            "cpuTimeLimit",
            "networkIsolation",
        ] {
            let e = all
                .iter()
                .find(|e| e.item == key)
                .unwrap_or_else(|| panic!("missing {key}"));
            assert!(!e.limit.is_empty(), "{key} must state its limit");
        }
        assert!(
            !all.iter()
                .find(|e| e.item == "filesystemConfinement")
                .unwrap()
                .enforced
        );
        assert!(
            !all.iter()
                .find(|e| e.item == "networkIsolation")
                .unwrap()
                .enforced
        );
    }

    #[test]
    fn refusal_names_the_opt_in_and_keeps_the_platform_reason() {
        let m = refusal_message();
        assert!(m.contains("Linux bubblewrap"), "{m}");
        assert!(m.contains("renderer was not started"), "{m}");
        assert!(m.contains(ALLOW_ENV), "{m}");
        assert!(m.contains(UNVERIFIED_REASON), "{m}");
    }

    #[test]
    fn argv_translation_maps_the_sandbox_prefixes_only() {
        let dir = TempDir::new_in(&std::env::temp_dir(), "hae-unsbx-argv-").unwrap();
        let in_dir = dir.path().join("in");
        let out_dir = dir.path().join("out");
        let tmp = dir.path().join("tmp");
        fs::create_dir_all(&in_dir).unwrap();
        let argv = [
            "/in/rhwp",
            "export-svg",
            "/in/doc.hwpx",
            "-o",
            "/out",
            "--page",
            "0",
            "/tmp/scratch",
        ];
        let got: Vec<String> = translate_argv(&argv, &in_dir, &out_dir, &tmp, None)
            .unwrap()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(got[0], in_dir.join("rhwp").display().to_string());
        assert_eq!(got[1], "export-svg");
        assert_eq!(got[2], in_dir.join("doc.hwpx").display().to_string());
        assert_eq!(got[3], "-o");
        assert_eq!(got[4], out_dir.display().to_string());
        assert_eq!(got[5], "--page");
        assert_eq!(got[6], "0");
        assert_eq!(got[7], tmp.join("scratch").display().to_string());
    }

    #[test]
    fn argv_translation_refuses_host_paths_and_missing_fonts() {
        let dir = TempDir::new_in(&std::env::temp_dir(), "hae-unsbx-refuse-").unwrap();
        let e = translate_argv(
            &["/in/rhwp", "/etc/passwd"],
            dir.path(),
            dir.path(),
            dir.path(),
            None,
        )
        .unwrap_err();
        assert!(e.contains("host path"), "{e}");
        let e = translate_argv(
            &["/in/hae-raster", "--fonts-dir", "/fonts"],
            dir.path(),
            dir.path(),
            dir.path(),
            None,
        )
        .unwrap_err();
        assert!(e.contains("/fonts"), "{e}");
        // A prefix that is not a path boundary is not remapped.
        assert_eq!(
            translate_argv(&["/index"], dir.path(), dir.path(), dir.path(), None).unwrap_err(),
            "unsandboxed run may not reference the host path \"/index\""
        );
    }
}
