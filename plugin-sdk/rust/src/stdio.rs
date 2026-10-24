//! The two streams the protocol runs on. On Unix the program's standard output is pointed at its standard error before
//! anything else can print, and the original output is kept for the protocol, so a stray `println!` (or a library that
//! prints) ends up in the log and not in the middle of a JSON message.

use tokio::io::{AsyncRead, AsyncWrite};

pub type Input = Box<dyn AsyncRead + Unpin + Send>;
pub type Output = Box<dyn AsyncWrite + Unpin + Send>;

#[cfg(unix)]
pub fn claim() -> (Input, Output) {
    use std::os::fd::FromRawFd;

    // SAFETY: dup returns a new descriptor that nothing else owns; dup2 then makes fd 1 an alias of stderr. If dup fails
    // the plugin keeps its normal stdout and protocol and prints share one stream, which is the documented non-Unix case.
    unsafe {
        let protocol_fd = libc::dup(1);
        if protocol_fd >= 0 && libc::dup2(2, 1) >= 0 {
            let file = std::fs::File::from_raw_fd(protocol_fd);
            return (Box::new(tokio::io::stdin()), Box::new(tokio::fs::File::from_std(file)));
        }
    }
    (Box::new(tokio::io::stdin()), Box::new(tokio::io::stdout()))
}

#[cfg(not(unix))]
pub fn claim() -> (Input, Output) {
    (Box::new(tokio::io::stdin()), Box::new(tokio::io::stdout()))
}
