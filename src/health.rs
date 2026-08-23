use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::config;
use crate::logger::CesarLogger;

pub struct HealthReport {
    pub errors: Vec<HealthIssue>,
    pub warnings: Vec<HealthIssue>,
    pub kernel: String,
    pub uptime_secs: f64,
    pub hostname: String,
    pub total_configs: usize,
    pub valid_configs: usize,
    pub zombies: usize,
    pub disk_used_pct: u64,
    pub disk_avail: String,
    pub tcp_listeners: usize,
}

pub struct HealthIssue {
    pub context: String,
    pub fix: String,
}

impl HealthReport {
    pub fn run() -> Self {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut total_configs = 0usize;
        let mut valid_configs = 0usize;
        let mut zombies = 0usize;


        let kernel = fs::read_to_string("/proc/version")
            .map(|v| v.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
            .unwrap_or_else(|_| "unknown".to_string());


        let uptime_secs = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0);


        let hostname = crate::hostname();


        let proc_ok = Path::new("/proc/uptime").exists();
        let sys_ok = Path::new("/sys/kernel").exists();
        let dev_ok = Path::new("/dev/null").exists();
        if !proc_ok || !sys_ok || !dev_ok {
            let mut missing = Vec::new();
            if !proc_ok { missing.push("/proc"); }
            if !sys_ok { missing.push("/sys"); }
            if !dev_ok { missing.push("/dev"); }
            errors.push(HealthIssue {
                context: format!("Virtual filesystems not mounted: {}", missing.join(", ")),
                fix: "mount -t proc proc /proc; mount -t sysfs sysfs /sys; mount -t devtmpfs devtmpfs /dev".to_string(),
            });
        }


        let cs_path = "/system/bin/csr";
        let init_path = "/sbin/init";
        if Path::new(cs_path).exists() {
            if let Ok(meta) = fs::metadata(cs_path)
                && meta.permissions().mode() & 0o111 == 0 {
                    warnings.push(HealthIssue {
                        context: format!("{} is not executable", cs_path),
                        fix: format!("chmod +x {}", cs_path),
                    });
                }
        } else if Path::new(init_path).exists() {
            if let Ok(meta) = fs::metadata(init_path)
                && meta.permissions().mode() & 0o111 == 0 {
                    warnings.push(HealthIssue {
                        context: format!("{} is not executable", init_path),
                        fix: format!("chmod +x {}", init_path),
                    });
                }
        } else {
            warnings.push(HealthIssue {
                context: "Neither /system/bin/csr nor /sbin/init found".to_string(),
                fix: "ln /system/bin/csr /sbin/init".to_string(),
            });
        }


        let sys_svc = "/system/lib/cesar/services";
        let user_svc = "/etc/cesar/services";
        let enabled_dir = "/etc/cesar/enabled";
        let user_ok = Path::new(user_svc).exists();
        let enabled_ok = Path::new(enabled_dir).exists();
        if !user_ok {
            warnings.push(HealthIssue {
                context: format!("{} does not exist", user_svc),
                fix: format!("mkdir -p {}", user_svc),
            });
        }
        if user_ok && !enabled_ok {
            warnings.push(HealthIssue {
                context: format!("{} does not exist (optional: only services linked there are enabled)", enabled_dir),
                fix: format!("mkdir -p {}  # then symlink services/*.ini to enable selective boot", enabled_dir),
            });
        }


        for dir in &[sys_svc, user_svc] {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                        total_configs += 1;
                        let path_str = path.to_string_lossy().to_string();
                        match config::parse_cesar_config(&path_str) {
                            Ok(cfg) => {
                                valid_configs += 1;
                                if !Path::new(&cfg.exec).exists() {
                                    errors.push(HealthIssue {
                                        context: format!("{}: Exec '{}' not found", path.file_name().unwrap_or_default().to_string_lossy(), cfg.exec),
                                        fix: "Install the package providing this binary or fix the Exec= path".to_string(),
                                    });
                                } else if let Ok(meta) = fs::metadata(&cfg.exec)
                                    && meta.permissions().mode() & 0o111 == 0 {
                                        warnings.push(HealthIssue {
                                            context: format!("{}: Exec '{}' not executable", path.file_name().unwrap_or_default().to_string_lossy(), cfg.exec),
                                            fix: format!("chmod +x {}", cfg.exec),
                                        });
                                    }
                                for dep in &cfg.requires {
                                    let dep_exists = [sys_svc, user_svc].iter().any(|d| {
                                        Path::new(&format!("{}/{}.ini", d, dep)).exists()
                                    });
                                    if !dep_exists {
                                        errors.push(HealthIssue {
                                            context: format!("{}: dependency '{}' not defined", path.file_name().unwrap_or_default().to_string_lossy(), dep),
                                            fix: format!("Create /etc/cesar/services/{}.ini or install the package", dep),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                errors.push(HealthIssue {
                                    context: format!("{}: {}", path.file_name().unwrap_or_default().to_string_lossy(), e),
                                    fix: "Fix the service config file or reinstall the package".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }


        if let Ok(proc_dir) = fs::read_dir("/proc") {
            let own_pid = unsafe { libc::getpid() };
            for entry in proc_dir.flatten() {
                let name = entry.file_name();
                if let Some(pid_str) = name.to_str()
                    && pid_str.chars().all(|c| c.is_ascii_digit()) {
                        // Our own children are reaped by PID 1; a transient
                        // zombie there is normal, not a system problem.
                        let ppid = fs::read_to_string(format!("/proc/{}/stat", pid_str))
                            .ok()
                            .and_then(|stat| {
                                stat.rsplit_once(')').and_then(|(_, rest)| {
                                    rest.split_whitespace().nth(1)?.parse::<i64>().ok()
                                })
                            });
                        if ppid == Some(own_pid as i64) {
                            continue;
                        }
                        let status_path = format!("/proc/{}/status", pid_str);
                        if let Ok(status) = fs::read_to_string(&status_path) {
                            for line in status.lines() {
                                if line.starts_with("State:") && line.contains('Z') {
                                    zombies += 1;
                                }
                            }
                        }
                    }
            }
        }
        if zombies > 0 {
            warnings.push(HealthIssue {
                context: format!("{} zombie process(es) detected", zombies),
                fix: "PID 1 (cesar) auto-reaps zombies. If persistent, check SIGCHLD handler".to_string(),
            });
        }


        let mut disk_used_pct = 0u64;
        let mut disk_avail = "unknown".to_string();
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c"/".as_ptr().cast(), &mut stat) == 0 {
                let total = stat.f_blocks * stat.f_frsize;
                // f_bfree (not f_bavail): init runs as root, so all free
                // blocks are usable and "used" is measured against them.
                let avail = stat.f_bfree * stat.f_frsize;
                disk_used_pct = (total - avail).checked_mul(100).and_then(|v| v.checked_div(total)).unwrap_or(0);
                disk_avail = format_bytes(avail);
            }
        }
        if disk_used_pct > 95 {
            errors.push(HealthIssue {
                context: format!("Root filesystem {}% full", disk_used_pct),
                fix: "Remove unused packages, clear /var/cache, or expand the partition".to_string(),
            });
        } else if disk_used_pct > 85 {
            warnings.push(HealthIssue {
                context: format!("Root filesystem {}% full", disk_used_pct),
                fix: "Consider freeing space before it becomes critical".to_string(),
            });
        }


        let tcp_listeners = fs::read_to_string("/proc/net/tcp")
            .map(|tcp| {
                tcp.lines().filter(|l| {
                    let fields: Vec<&str> = l.split_whitespace().collect();
                    fields.len() > 3 && fields[3] == "0A"
                }).count()
            })
            .unwrap_or(0);


        let log_file = "/var/log/cesar.md";
        if !Path::new(log_file).exists()

            && Path::new("/var/log").exists() {
                warnings.push(HealthIssue {
                    context: format!("{} does not exist yet", log_file),
                    fix: "Created automatically on first boot by CesarLogger".to_string(),
                });
            }

        HealthReport {
            errors,
            warnings,
            kernel,
            uptime_secs,
            hostname,
            total_configs,
            valid_configs,
            zombies,
            disk_used_pct,
            disk_avail,
            tcp_listeners,
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn log_issues(&self, logger: &CesarLogger) {
        for issue in &self.errors {
            logger.log_error("health", &format!("{} — Fix: {}", issue.context, issue.fix));
        }
        for issue in &self.warnings {
            logger.log_warning("health", &format!("{} — Fix: {}", issue.context, issue.fix));
        }
    }

    pub fn print_status(&self) {
        println!("\x1b[36mHealth:\x1b[0m");


        println!("  Host:    {}", self.hostname);
        println!("  Kernel:  {}", self.kernel);
        let days = (self.uptime_secs / 86400.0) as u64;
        let hours = ((self.uptime_secs % 86400.0) / 3600.0) as u64;
        let mins = ((self.uptime_secs % 3600.0) / 60.0) as u64;
        if days > 0 {
            println!("  Uptime:  {}d {}h {}m", days, hours, mins);
        } else if hours > 0 {
            println!("  Uptime:  {}h {}m", hours, mins);
        } else {
            println!("  Uptime:  {}m", mins);
        }


        println!("  Disk:    {}% used ({} free)", self.disk_used_pct, self.disk_avail);


        println!("  TCP:     {} listener(s)", self.tcp_listeners);


        if self.total_configs > 0 {
            println!("  Configs: {}/{} valid", self.valid_configs, self.total_configs);
        } else {
            println!("  Configs: none found");
        }


        if self.zombies > 0 {
            println!("  Zombies: \x1b[33m{}\x1b[0m", self.zombies);
        }


        let proc_ok = Path::new("/proc/uptime").exists();
        let sys_ok = Path::new("/sys/kernel").exists();
        let dev_ok = Path::new("/dev/null").exists();
        let fs_status = match (proc_ok, sys_ok, dev_ok) {
            (true, true, true) => "\x1b[32mok\x1b[0m",
            _ => "\x1b[31mdegraded\x1b[0m",
        };
        println!("  FS:      {}", fs_status);


        if !self.errors.is_empty() || !self.warnings.is_empty() {
            println!();
            if !self.errors.is_empty() {
                println!("  \x1b[31m{} error(s):\x1b[0m", self.errors.len());
                for (i, issue) in self.errors.iter().enumerate() {
                    println!("    {}. [CTX] {}", i + 1, issue.context);
                    println!("       [FIX] {}", issue.fix);
                }
            }
            if !self.warnings.is_empty() {
                println!("  \x1b[33m{} warning(s):\x1b[0m", self.warnings.len());
                for (i, issue) in self.warnings.iter().enumerate() {
                    println!("    {}. [CTX] {}", i + 1, issue.context);
                    println!("       [FIX] {}", issue.fix);
                }
            }
        } else {
            println!("  \x1b[32m✓ All checks passed\x1b[0m");
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
