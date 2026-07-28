use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use chrono::Local;

const LOG_PATH: &str = "/var/log/cesar.md";

pub struct CesarLogger {
    pub log_path: String,
}

impl CesarLogger {
    pub fn new() -> Self {
        let logger = CesarLogger {
            log_path: LOG_PATH.to_string(),
        };
        logger.init_header();
        logger
    }

    fn init_header(&self) {
        if let Some(parent) = Path::new(&self.log_path).parent() {
            fs::create_dir_all(parent).ok();
        }
        if Path::new(&self.log_path).exists() {
            return;
        }
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "Cudane-Core".to_string());
        let now = Local::now().format("%Y-%m-%d");
        let header = format!(
            "# ─── CESAR SYSTEM LOGS SUMMARY ───\nDate: {} | Host: {}\n\n",
            now, hostname
        );
        fs::write(&self.log_path, &header).ok();
    }

    pub fn clear_log(&self) {
        if let Some(parent) = Path::new(&self.log_path).parent() {
            fs::create_dir_all(parent).ok();
        }
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "Cudane-Core".to_string());
        let now = Local::now().format("%Y-%m-%d");
        let header = format!(
            "# ─── CESAR SYSTEM LOGS SUMMARY ───\nDate: {} | Host: {}\n\n",
            now, hostname
        );
        fs::write(&self.log_path, &header).ok();
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
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .ok();

        if let Some(ref mut f) = file {
            f.write_all(content.as_bytes()).ok();
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
