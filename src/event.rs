use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixDatagram;
use std::path::Path;

const EVENT_SOCKET: &str = "/run/cesar/event.sock";

pub struct EventBus;

impl EventBus {
    fn emit(event: &str, data: &serde_json::Value) {
        let msg = serde_json::json!({"event": event, "data": data});
        if let Ok(sock) = UnixDatagram::unbound() {
            let _ = sock.send_to(msg.to_string().as_bytes(), EVENT_SOCKET);
        }
    }

    pub fn emit_kv(event: &str, key: &str, value: &str) {
        Self::emit(event, &serde_json::json!({key: value}));
    }

    pub fn emit_service(name: &str, state: &str, pid: u32) {
        Self::emit(
            "service",
            &serde_json::json!({
                "name": name, "state": state, "pid": pid,
            }),
        );
    }

    pub fn emit_boot(total: usize, failed: usize) {
        Self::emit(
            "boot",
            &serde_json::json!({
                "total_services": total, "failed_services": failed,
            }),
        );
    }

    pub fn emit_shutdown() {
        Self::emit("shutdown", &serde_json::Value::Null);
    }

    pub fn start_listener() -> Option<UnixDatagram> {
        let path = Path::new(EVENT_SOCKET);
        if let Ok(meta) = std::fs::metadata(path)
            && meta.file_type().is_socket() {
                let _ = std::fs::remove_file(path);
            }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match UnixDatagram::bind(path) {
            Ok(sock) => {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660));
                Some(sock)
            }
            Err(_) => None,
        }
    }
}
