use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use chrono::Local;

const LOG_PATH: &str = "/var/log/cesar.md";
/// Rotate the active log to `<log>.1` once it exceeds this size.
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub struct CesarLogger {
    pub log_path: String,
    /// Held-open append handle; `None` only when the log was never
    /// openable (e.g. read-only /var from a CLI context).
    file: Mutex<Option<File>>,
    /// Write failures are surfaced exactly once instead of being
    /// silently swallowed on every call.
    write_failed: AtomicBool,
}

impl CesarLogger {
    pub fn new() -> Self {
        let logger = CesarLogger {
            log_path: LOG_PATH.to_string(),
            file: Mutex::new(None),
            write_failed: AtomicBool::new(false),
        };
        logger.init_header();
        logger
    }

    /// Logger writing to an arbitrary path (used by self-tests so they
    /// never touch the production log).
    pub fn at_path(path: &str) -> Self {
        let logger = CesarLogger {
            log_path: path.to_string(),
            file: Mutex::new(None),
            write_failed: AtomicBool::new(false),
        };
        logger.init_header();
        logger
    }

    fn header_text() -> String {
        let hostname = crate::hostname();
        let host = if hostname.is_empty() {
            "Cudane-Core".to_string()
        } else {
            hostname
        };
        format!(
            "# ─── CESAR SYSTEM LOGS SUMMARY ───\nDate: {} | Host: {}\n\n",
            Local::now().format("%Y-%m-%d"),
            host
        )
    }

    fn init_header(&self) {
        if let Some(parent) = Path::new(&self.log_path).parent() {
            fs::create_dir_all(parent).ok();
        }
        if !Path::new(&self.log_path).exists() {
            fs::write(&self.log_path, Self::header_text()).ok();
        }
        // Logs may capture service output context: keep them private to
        // root/group regardless of the creating process's umask.
        let _ = fs::set_permissions(&self.log_path, fs::Permissions::from_mode(0o640));
        self.open_file();
    }

    fn open_file(&self) {
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            *guard = Self::open_append(&self.log_path);
        }
    }

    fn open_append(path: &str) -> Option<File> {
        OpenOptions::new().create(true).append(true).open(path).ok()
    }

    pub fn clear_log(&self) {
        if let Some(parent) = Path::new(&self.log_path).parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        *guard = None;
        fs::write(&self.log_path, Self::header_text()).ok();
        let _ = fs::set_permissions(&self.log_path, fs::Permissions::from_mode(0o640));
        *guard = Self::open_append(&self.log_path);
    }

    pub fn log_info(&self, service: &str, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S");
        let entry = format!("## [{}]\n> [{}] INFO: {}\n\n", service, timestamp, message);
        self.append(&entry);
    }

    pub fn log_error(&self, service: &str, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S");
        let entry = format!(
            "## [{}]\n> [{}] CRITICAL: {}\n\n",
            service, timestamp, message
        );
        self.append(&entry);
    }

    pub fn log_warning(&self, service: &str, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S");
        let entry = format!(
            "## [{}]\n> [{}] WARNING: {}\n\n",
            service, timestamp, message
        );
        self.append(&entry);
    }

    pub fn log_boot_start(&self) {
        let timestamp = Local::now().format("%H:%M:%S");
        let entry = format!(
            "# ─── CESAR BOOT SEQUENCE ───\n> [{}] Boot initiated\n\n",
            timestamp
        );
        self.append(&entry);
    }

    pub fn log_boot_complete(&self, total_services: usize, failed: usize) {
        let timestamp = Local::now().format("%H:%M:%S");
        let entry = format!(
            "# ─── CESAR BOOT COMPLETE ───\n> [{}] {} services started, {} failed\n\n",
            timestamp,
            total_services - failed,
            failed
        );
        self.append(&entry);
    }

    pub fn log_service_event(&self, service: &str, event: &str) {
        let timestamp = Local::now().format("%H:%M:%S");
        let entry = format!("## [{}]\n> [{}] EVENT: {}\n\n", service, timestamp, event);
        self.append(&entry);
    }

    fn append(&self, content: &str) {
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);

        // Size-cap rotation: close the handle, rename, reopen fresh.
        // The rotated copy inherits its permissions at rename time.
        let size = fs::metadata(&self.log_path).map(|m| m.len()).unwrap_or(0);
        if size > LOG_MAX_BYTES {
            *guard = None;
            fs::rename(&self.log_path, format!("{}.1", self.log_path)).ok();
            if let Ok(dir) = fs::File::open(
                Path::new(&self.log_path)
                    .parent()
                    .unwrap_or_else(|| Path::new(".")),
            ) {
                let _ = dir.sync_all();
            }
            *guard = Self::open_append(&self.log_path);
            if guard.is_some() {
                let _ = fs::set_permissions(&self.log_path, fs::Permissions::from_mode(0o640));
            }
        } else if guard.is_none() {
            *guard = Self::open_append(&self.log_path);
        }

        match guard.as_mut() {
            Some(f) => match f.write_all(content.as_bytes()) {
                Ok(()) => {
                    self.write_failed.store(false, Ordering::SeqCst);
                }
                Err(_) => self.report_write_failure(),
            },
            None => self.report_write_failure(),
        }
    }

    fn report_write_failure(&self) {
        if !self.write_failed.swap(true, Ordering::SeqCst) {
            eprintln!(
                "[Warning] :: cannot write to {}; logging disabled",
                self.log_path
            );
        }
    }

    pub fn read_log(&self) -> String {
        fs::read_to_string(&self.log_path).unwrap_or_default()
    }
}

impl Default for CesarLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the slice of `s` starting at the largest char boundary `>= from`
/// that is a valid boundary, plus the end offset of the printable region.
/// Prevents panics when a multi-byte character is split across reads
/// while tailing a growing file.
pub fn safe_tail(s: &str, from: usize) -> (&str, usize) {
    let mut start = from.min(s.len());
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    let mut end = s.len();
    while end > start && !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[start..end], end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_reads_back() {
        let path = format!("/tmp/opencode/cesar-log-test-{}.md", std::process::id());
        let _ = fs::remove_file(&path);
        let logger = CesarLogger::at_path(&path);
        logger.log_info("svc-a", "hello info");
        logger.log_error("svc-b", "boom");
        let content = logger.read_log();
        assert!(content.contains("INFO: hello info"));
        assert!(content.contains("CRITICAL: boom"));
        assert!(content.contains("[svc-a]"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn log_file_created_with_0640_mode() {
        let path = format!("/tmp/opencode/cesar-log-mode-{}.md", std::process::id());
        let _ = fs::remove_file(&path);
        let logger = CesarLogger::at_path(&path);
        logger.log_info("svc", "mode check");
        let mode = fs::metadata(&path)
            .expect("log exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o640);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn clear_log_resets_to_header_only() {
        let path = format!("/tmp/opencode/cesar-log-clear-{}.md", std::process::id());
        let logger = CesarLogger::at_path(&path);
        logger.log_info("svc", "to be cleared");
        assert!(logger.read_log().contains("to be cleared"));
        logger.clear_log();
        assert!(!logger.read_log().contains("to be cleared"));
        assert!(logger.read_log().contains("CESAR SYSTEM LOGS SUMMARY"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn rotation_renames_oversized_log() {
        let path = format!("/tmp/opencode/cesar-log-rot-{}.md", std::process::id());
        let rotated = format!("{}.1", path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&rotated);
        fs::write(&path, "x".repeat(LOG_MAX_BYTES as usize + 1)).expect("seed oversized log");
        let logger = CesarLogger::at_path(&path);
        logger.log_info("svc", "post-rotation entry");
        assert!(Path::new(&rotated).exists(), "oversized log renamed to .1");
        assert!(logger.read_log().contains("post-rotation entry"));
        let fresh_len = fs::metadata(&path).expect("fresh log").len();
        assert!(fresh_len <= LOG_MAX_BYTES);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&rotated);
    }

    #[test]
    fn safe_tail_handles_multibyte_boundaries() {
        let s = "héllo wörld";
        let (tail, end) = safe_tail(s, 2); // byte 2 sits inside 'é' (bytes 1..3)
        assert!(s[end..].is_empty());
        assert_eq!(tail, "llo wörld"); // resumed at the next valid boundary
    }
}
