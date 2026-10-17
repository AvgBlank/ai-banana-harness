//! Keeps stdout for protocol frames only.

use std::fs::File;
use std::io;
use std::os::fd::FromRawFd;

/// Moves the real stdout to a private descriptor and points fd 1 at stderr, so anything else
/// that prints to stdout (a dependency, a stray `println!`) lands in the logs, not the stream.
/// Returns the file the protocol must be written to. Call it before anything else runs.
// ponytail: unix only; a Windows port needs the equivalent handle swap.
pub fn claim_stdout() -> io::Result<File> {
    // SAFETY: plain descriptor duplication; the new descriptor is owned by the returned File.
    unsafe {
        let private = libc::dup(1);
        if private < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::dup2(2, 1) < 0 {
            let error = io::Error::last_os_error();
            libc::close(private);
            return Err(error);
        }
        Ok(File::from_raw_fd(private))
    }
}
