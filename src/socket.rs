use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Listening sockets pre-created by PID 1, keyed by service name.
/// Children receive them via dup2 onto fds 3.. (sd_listen_fds convention).
static SERVICE_LISTEN_FDS: OnceLock<Mutex<HashMap<String, Vec<i32>>>> = OnceLock::new();

fn listen_registry() -> &'static Mutex<HashMap<String, Vec<i32>>> {
    SERVICE_LISTEN_FDS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_listen_fd(service: &str, fd: i32) {
    listen_registry()
        .lock()
        .expect("socket registry lock")
        .entry(service.to_string())
        .or_default()
        .push(fd);
}

/// Copy of the listening fds for `service` (parent side; originals stay open).
pub fn listen_fds_for(service: &str) -> Vec<i32> {
    listen_registry()
        .lock()
        .expect("socket registry lock")
        .get(service)
        .cloned()
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SocketConfig {
    pub service_name: String,
    pub socket_path: String,
    pub socket_type: SocketType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SocketType {
    UnixStream,
    UnixDatagram,
    Tcp,
}

pub struct SocketActivator {
    pub sockets: HashMap<String, SocketConfig>,
}

impl Default for SocketActivator {
    fn default() -> Self {
        Self::new()
    }
}

impl SocketActivator {
    pub fn new() -> Self {
        SocketActivator {
            sockets: HashMap::new(),
        }
    }

    pub fn register(&mut self, service_name: &str, socket_path: &str, sock_type: SocketType) {
        self.sockets.insert(
            socket_path.to_string(),
            SocketConfig {
                service_name: service_name.to_string(),
                socket_path: socket_path.to_string(),
                socket_type: sock_type,
            },
        );
    }

    pub fn parse_socket_spec(spec: &str) -> Option<(String, SocketType)> {
        let spec = spec.trim();
        if let Some(path) = spec.strip_prefix("unix:") {
            let path = path.trim();
            if path.ends_with(".sock") {
                Some((path.to_string(), SocketType::UnixStream))
            } else {
                Some((path.to_string(), SocketType::UnixDatagram))
            }
        } else if let Some(addr) = spec.strip_prefix("tcp:") {
            Some((addr.trim().to_string(), SocketType::Tcp))
        } else {
            Some((spec.to_string(), SocketType::UnixStream))
        }
    }
}

/// Create a listening socket for `path` of `sock_type` and return its fd.
/// The fd stays owned by PID 1 (CLOEXEC) until handed to the service child.
pub fn create_socket_for(path: &str, sock_type: &SocketType) -> Result<i32, String> {
    match sock_type {
        SocketType::UnixStream => create_unix_socket(path),
        SocketType::UnixDatagram => create_unix_datagram(path),
        SocketType::Tcp => create_tcp_socket(path),
    }
}

/// Create a listening TCP socket. `addr` is "host:port" or ":port".
fn create_tcp_socket(addr: &str) -> Result<i32, String> {
    use std::net::TcpListener;
    use std::os::fd::IntoRawFd;

    let listener = TcpListener::bind(addr)
        .map_err(|e| format!("tcp bind '{}' failed: {}", addr, e))?;
    Ok(listener.into_raw_fd())
}

/// Bind an AF_UNIX socket of `type` to `path` safely.
///
/// The socket is bound to a temporary sibling name and renamed onto `path`
/// atomically: a live socket already at `path` keeps working until the
/// instant of the swap (no unlinked-but-bound inode left behind), and a
/// symlink planted at `path` is replaced wholesale instead of followed.
fn bind_unix_socket(path: &str, sock_type: libc::c_int) -> Result<i32, String> {
    use std::ffi::CString;

    let sock_path = std::path::Path::new(path);
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {}", parent.display(), e))?;
    }

    let c_path = CString::new(path).map_err(|e| format!("Invalid path: {}", e))?;

    // Temporary bind address in the same directory (rename(2) requires it).
    let file_name = sock_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "socket".to_string());
    let parent = sock_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let tmp_path = parent.join(format!(".{}.bind.{}", file_name, std::process::id()));
    let _ = std::fs::remove_file(&tmp_path);
    let c_tmp = CString::new(tmp_path.to_string_lossy().as_bytes())
        .map_err(|e| format!("Invalid temp path: {}", e))?;

    unsafe {
        let sock = libc::socket(libc::AF_UNIX, sock_type | libc::SOCK_CLOEXEC, 0);
        if sock < 0 {
            return Err("Socket creation failed".to_string());
        }

        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        if c_tmp.to_bytes().len() >= addr.sun_path.len() {
            libc::close(sock);
            return Err("Socket path too long".to_string());
        }
        std::ptr::copy_nonoverlapping(
            c_tmp.to_bytes().as_ptr(),
            addr.sun_path.as_mut_ptr() as *mut u8,
            c_tmp.to_bytes().len(),
        );

        let addr_ptr = &addr as *const libc::sockaddr_un as *const libc::sockaddr;
        let addr_len = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;

        if libc::bind(sock, addr_ptr, addr_len) < 0 {
            libc::close(sock);
            return Err(format!("Socket bind failed: {}", std::io::Error::last_os_error()));
        }

        if sock_type == libc::SOCK_STREAM && libc::listen(sock, 128) < 0 {
            libc::close(sock);
            let _ = std::fs::remove_file(&tmp_path);
            return Err("Socket listen failed".to_string());
        }

        // Atomic swap onto the public name; clean up the temp inode if the
        // rename somehow fails so no stray dotfile lingers.
        if libc::rename(c_tmp.as_ptr(), c_path.as_ptr()) < 0 {
            let err = std::io::Error::last_os_error();
            libc::close(sock);
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Socket publish failed: {}", err));
        }

        // Sockets are service endpoints: group-writable, not world.
        // (fchmod(2) is a silent no-op on socket inodes; chmod the path.)
        libc::chmod(c_path.as_ptr(), 0o660);

        Ok(sock)
    }
}

/// Create a bound AF_UNIX datagram socket; returns its fd.
pub fn create_unix_datagram(path: &str) -> Result<i32, String> {
    bind_unix_socket(path, libc::SOCK_DGRAM)
}

/// Create a listening AF_UNIX stream socket; returns its fd.
pub fn create_unix_socket(path: &str) -> Result<i32, String> {
    bind_unix_socket(path, libc::SOCK_STREAM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_socket_spec_unix_stream() {
        let (path, kind) = SocketActivator::parse_socket_spec("unix:/run/x.sock").expect("parses");
        assert_eq!(path, "/run/x.sock");
        assert_eq!(kind, SocketType::UnixStream);
    }

    #[test]
    fn parse_socket_spec_unix_datagram() {
        let (path, kind) = SocketActivator::parse_socket_spec("unix:/run/y").expect("parses");
        assert_eq!(path, "/run/y");
        assert_eq!(kind, SocketType::UnixDatagram);
    }

    #[test]
    fn parse_socket_spec_tcp() {
        let (addr, kind) = SocketActivator::parse_socket_spec("tcp:127.0.0.1:8080").expect("parses");
        assert_eq!(addr, "127.0.0.1:8080");
        assert_eq!(kind, SocketType::Tcp);
    }

    #[test]
    fn parse_socket_spec_defaults_to_unix_stream() {
        let (path, kind) = SocketActivator::parse_socket_spec("/run/plain.sock").expect("parses");
        assert_eq!(path, "/run/plain.sock");
        assert_eq!(kind, SocketType::UnixStream);
    }

    #[test]
    fn unix_datagram_socket_created_with_0660_mode() {
        let path = format!("/tmp/opencode/cesar-dgram-{}.sock", std::process::id());
        let _ = std::fs::remove_file(&path);
        let fd = create_unix_datagram(&path).expect("socket created");
        unsafe { libc::close(fd); }
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("exists").permissions().mode();
        assert_eq!(mode & 0o777, 0o660);
        let _ = std::fs::remove_file(&path);
    }
}
