//! Rooted I/O, session-owned project locks, atomic saves and HWPX export.
//!
//! On Unix, paths are absolute and lexically canonical (no `.`, `..`, repeated
//! slashes, symlinks or hard links); roots must already exist. Traversal uses
//! directory descriptors and `O_NOFOLLOW`. On Windows, the std::fs backend
//! rejects reparse points and performs per-component checks, but cannot provide
//! the same race-free dirfd guarantee; see `io/windows.rs` and the dependency
//! decision for its limits. Keep one [`IoRoots`] for the executor so every
//! HWPX/image input is registered as protected. Keep the returned
//! [`ProjectBinding`] until session close.
//!
//! Every write requires an independent validation callback. On project saves it
//! receives the **current HWPX extracted from the temporary project**, on export
//! it receives the temporary HWPX. Callback errors prevent publication.
//!
//! Advisory locks coordinate cooperating processes. Directory/lock/target
//! identity is rechecked immediately before publication, but Linux has no atomic
//! compare-hash-and-rename: hostile writers with directory access can still race
//! the final check. Use roots writable only by trusted cooperating processes.

mod format;
mod io;

pub use format::ProjectManifest;
pub use io::{FaultStep, InputFile, IoRoots, OpenedProject, ProjectBinding, SaveReceipt};

#[cfg(test)]
mod tests;
