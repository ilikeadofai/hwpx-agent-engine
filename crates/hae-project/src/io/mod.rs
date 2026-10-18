#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{FaultStep, InputFile, IoRoots, OpenedProject, ProjectBinding, SaveReceipt};
#[cfg(windows)]
pub use windows::{FaultStep, InputFile, IoRoots, OpenedProject, ProjectBinding, SaveReceipt};

#[cfg(not(any(unix, windows)))]
compile_error!("hae-project I/O is supported only on Unix and Windows targets");
