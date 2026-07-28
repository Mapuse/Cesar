use std::collections::HashMap;

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

pub fn create_unix_socket(path: &str) -> Result<(), String> {
    use std::ffi::CString;

    let sock_path = std::path::Path::new(path);
    if sock_path.exists() {
        std::fs::remove_file(sock_path).ok();
    }

    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    unsafe {
        let sock = libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
            0,
        );
        if sock < 0 {
            return Err("Socket creation failed".to_string());
        }

        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let c_path = CString::new(path).map_err(|e| format!("Invalid path: {}", e))?;
        let bytes = c_path.to_bytes();
        if bytes.len() >= addr.sun_path.len() {
            libc::close(sock);
            return Err("Socket path too long".to_string());
        }
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            addr.sun_path.as_mut_ptr() as *mut u8,
            bytes.len(),
        );

        let addr_ptr = &addr as *const libc::sockaddr_un as *const libc::sockaddr;
        let addr_len = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;

        if libc::bind(sock, addr_ptr, addr_len) < 0 {
            libc::close(sock);
            return Err("Socket bind failed".to_string());
        }

        if libc::listen(sock, 128) < 0 {
            libc::close(sock);
            return Err("Socket listen failed".to_string());
        }
    }

    Ok(())
}
