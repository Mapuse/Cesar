pub mod cli;
pub mod commands;
pub mod config;
pub mod dag;
pub mod event;
pub mod health;
pub mod ipc;
pub mod logger;
pub mod process;
pub mod service;
pub mod socket;
pub mod visual;

/// System hostname via gethostname(2).
pub fn hostname() -> String {
    const MAX_HOST_NAME: usize = 256; // >= HOST_NAME_MAX on every supported target
    let mut buf = [0u8; MAX_HOST_NAME];
    let ret = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if ret == 0 {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        if end > 0 {
            return String::from_utf8_lossy(&buf[..end]).into_owned();
        }
    }
    String::from("unknown")
}
