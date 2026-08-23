//! Control-socket IPC between the running init (PID 1) and `csr` CLI calls.
//!
//! PID 1 listens on a Unix stream socket; CLI commands forward requests
//! there so they act on the *live* service state instead of re-parsing
//  config files from disk.
//!
//! Protocol: one request line, then the server replies with
//!   "OK <exit_code>" or "ERR <message>" as the first line,
//! optionally followed by payload lines, then closes the connection.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub const CONTROL_SOCKET: &str = "/run/cesar/control.sock";

#[derive(Debug, Clone, PartialEq)]
pub enum ControlAction {
    Ping,
    List,
    Status(String),
    Start(String),
    Stop(String),
    Restart(String),
    Shutdown,
    Reboot,
    Poweroff,
}

impl ControlAction {
    /// Wire name for this action (first token of a request line).
    pub fn name(&self) -> &'static str {
        match self {
            ControlAction::Ping => "ping",
            ControlAction::List => "list",
            ControlAction::Status(_) => "status",
            ControlAction::Start(_) => "start",
            ControlAction::Stop(_) => "stop",
            ControlAction::Restart(_) => "restart",
            ControlAction::Shutdown => "shutdown",
            ControlAction::Reboot => "reboot",
            ControlAction::Poweroff => "poweroff",
        }
    }

    /// Mutating/system actions require root credentials (SO_PEERCRED);
    /// read-only queries stay open to local unprivileged callers.
    pub fn is_privileged(&self) -> bool {
        matches!(
            self,
            ControlAction::Start(_)
                | ControlAction::Stop(_)
                | ControlAction::Restart(_)
                | ControlAction::Shutdown
                | ControlAction::Reboot
                | ControlAction::Poweroff
        )
    }
}

/// Free-function wrappers kept for call-site brevity.
pub fn action_name(action: &ControlAction) -> &'static str {
    action.name()
}

pub fn action_is_privileged(action: &ControlAction) -> bool {
    action.is_privileged()
}

pub fn parse_request(line: &str) -> Option<ControlAction> {
    let line = line.trim();
    let mut parts = line.splitn(2, ' ');
    let verb = parts.next()?;
    let arg = parts.next().map(|s| s.trim().to_string());
    match verb {
        "ping" => Some(ControlAction::Ping),
        "list" => Some(ControlAction::List),
        "status" => arg.map(ControlAction::Status),
        "start" => arg.map(ControlAction::Start),
        "stop" => arg.map(ControlAction::Stop),
        "restart" => arg.map(ControlAction::Restart),
        "shutdown" => Some(ControlAction::Shutdown),
        "reboot" => Some(ControlAction::Reboot),
        "poweroff" => Some(ControlAction::Poweroff),
        _ => None,
    }
}

/// Try to reach the running init's control socket. On success returns the
/// response body (without the status line) plus its exit code.
pub fn client_request(action: &ControlAction) -> Result<(String, i32), String> {
    request_at(CONTROL_SOCKET, action)
}

/// Wire-format core of [`client_request`] against an explicit socket path
/// (split out so tests can exercise real framing on a temporary socket).
fn request_at(path: &str, action: &ControlAction) -> Result<(String, i32), String> {
    let line = request_line(action);

    let mut stream = UnixStream::connect(path)
        .map_err(|e| format!("no control socket at {}: {}", path, e))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    stream
        .write_all(line.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|e| format!("write failed: {}", e))?;
    stream.flush().ok();

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("read failed: {}", e))?;
    let status_line = status_line.trim();
    let (code, ok) = if let Some(rest) = status_line.strip_prefix("OK ") {
        (rest.parse().unwrap_or(0), true)
    } else if status_line.starts_with("ERR") {
        (-1, false)
    } else {
        return Err(format!("malformed response: {:?}", status_line));
    };

    let mut body = String::new();
    reader.read_to_string(&mut body).map_err(|e| format!("read failed: {}", e))?;
    if !ok && body.is_empty() {
        return Err(status_line.trim_start_matches("ERR ").to_string());
    }
    Ok((body, code))
}

/// The single request line sent for `action`.
fn request_line(action: &ControlAction) -> String {
    match action {
        ControlAction::Ping => "ping".to_string(),
        ControlAction::List => "list".to_string(),
        ControlAction::Status(n) => format!("status {}", n),
        ControlAction::Start(n) => format!("start {}", n),
        ControlAction::Stop(n) => format!("stop {}", n),
        ControlAction::Restart(n) => format!("restart {}", n),
        ControlAction::Shutdown => "shutdown".to_string(),
        ControlAction::Reboot => "reboot".to_string(),
        ControlAction::Poweroff => "poweroff".to_string(),
    }
}

/// True when the control socket exists (a live daemon is presumably
/// listening). Used to distinguish "forwarded and refused" from
/// "couldn't forward".
fn socket_present() -> bool {
    std::path::Path::new(CONTROL_SOCKET).exists()
}

/// Forward a CLI action to PID 1.
///
/// - Connected (any reply): print the reply and exit — nonzero with the
///   server's error message when the daemon answered ERR, so callers never
///   fall back to offline execution after an explicit refusal.
/// - Socket unreachable: returns `false` so callers may fall back to
///   offline behaviour.
pub fn try_forward(action: &ControlAction) -> bool {
    match client_request(action) {
        Ok((body, code)) => {
            print!("{}", body);
            std::process::exit(code);
        }
        Err(e) => {
            if socket_present() {
                eprintln!("[Error] :: {}", e);
                std::process::exit(1);
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_round_trips_all_actions() {
        let cases: Vec<(String, ControlAction)> = vec![
            ("ping".into(), ControlAction::Ping),
            ("list".into(), ControlAction::List),
            ("status web".into(), ControlAction::Status("web".into())),
            ("start db".into(), ControlAction::Start("db".into())),
            ("stop db".into(), ControlAction::Stop("db".into())),
            ("restart db".into(), ControlAction::Restart("db".into())),
            ("shutdown".into(), ControlAction::Shutdown),
            ("reboot".into(), ControlAction::Reboot),
            ("poweroff".into(), ControlAction::Poweroff),
        ];
        for (line, expected) in &cases {
            let parsed = parse_request(line).expect(line);
            assert_eq!(&parsed, expected);
            assert_eq!(request_line(&parsed), *line);
        }
    }

    #[test]
    fn parse_request_tolerates_whitespace_and_rejects_garbage() {
        assert_eq!(parse_request("  list \n"), Some(ControlAction::List));
        assert_eq!(parse_request(""), None);
        assert_eq!(parse_request("frobnicate"), None);
        // Verbs that require an argument must not parse without one.
        assert_eq!(parse_request("start"), None);
        assert_eq!(parse_request("status"), None);
    }

    #[test]
    fn privileged_actions_are_exactly_the_mutating_ones() {
        for action in [
            ControlAction::Start("x".into()),
            ControlAction::Stop("x".into()),
            ControlAction::Restart("x".into()),
            ControlAction::Shutdown,
            ControlAction::Reboot,
            ControlAction::Poweroff,
        ] {
            assert!(action.is_privileged(), "{} must be privileged", action.name());
        }
        for action in [ControlAction::Ping, ControlAction::List, ControlAction::Status("x".into())] {
            assert!(!action.is_privileged(), "{} must stay open", action.name());
        }
    }

    /// Minimal stand-in for the PID-1 side: reads one line, replies with a
    /// canned status line plus optional body, then closes — the exact
    /// framing `handle_control_conn` speaks.
    fn serve_one(path: &str, reply: &'static str) {
        let _ = std::fs::remove_file(path);
        let listener = std::os::unix::net::UnixListener::bind(path).expect("bind");
        let cleanup = path.to_string();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                use std::io::BufRead;
                let mut reader = std::io::BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() && !line.trim().is_empty() {
                    use std::io::Write;
                    let mut w = reader.get_ref().try_clone().expect("clone");
                    let _ = writeln!(w, "{}", reply);
                }
            }
            let _ = std::fs::remove_file(&cleanup);
        });
    }

    fn temp_sock(tag: &str) -> String {
        let dir = std::env::temp_dir();
        dir.join(format!("cesar-ipc-test-{}-{}.sock", tag, std::process::id()))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn framing_round_trip_ok_with_body() {
        let path = temp_sock("ok");
        serve_one(&path, "OK 0\nhello world");
        let (body, code) =
            request_at(&path, &ControlAction::Status("web".into())).expect("reply");
        assert_eq!(code, 0);
        assert_eq!(body, "hello world\n");
    }

    #[test]
    fn framing_round_trip_err_with_message_body() {
        let path = temp_sock("errbody");
        serve_one(&path, "ERR service 'web' not found");
        let err = request_at(&path, &ControlAction::Stop("web".into())).unwrap_err();
        assert!(err.contains("not found"), "got: {}", err);
    }

    #[test]
    fn framing_err_without_body_yields_status_text() {
        let path = temp_sock("errempty");
        serve_one(&path, "ERR permission denied for 'stop' (uid 1000): root required");
        let err = request_at(&path, &ControlAction::Stop("web".into())).unwrap_err();
        assert!(err.contains("permission denied"));
    }

    #[test]
    fn unreachable_socket_is_an_error_not_a_refusal() {
        let path = temp_sock("missing");
        let _ = std::fs::remove_file(&path);
        let msg = request_at(&path, &ControlAction::List).unwrap_err();
        assert!(msg.contains("no control socket"), "got: {}", msg);
    }
}
