use std::path::Path;

use crate::service::ServiceState;
use crate::dag::DagEngine;

pub const CESAR_BANNER: &str = r#"
 ██████╗███████╗███████╗ █████╗ ██████╗
██╔════╝██╔════╝██╔════╝██╔══██╗██╔══██╗
██║     █████╗  ███████╗███████║██████╔╝
██║     ██╔══╝ ╚════ ██║██╔══██║██╔══██╗
╚██████╗███████╗███████║██║  ██║██║  ██║
 ╚═════╝╚══════╝╚══════╝╚═╝  ╚═╝╚═╝   ╚═╝"#;

fn diagnose_service_failure(name: &str, error_msg: &str, dag: &DagEngine) -> Vec<(String, String)> {
    let mut diagnostics = Vec::new();

    if let Some(svc) = dag.services.get(name) {
        let exec = &svc.config.exec;

        if error_msg.contains("not found") || error_msg.contains("No such file") {
            if !Path::new(exec).exists() {
                diagnostics.push((
                    format!("Binary '{}' does not exist", exec),
                    format!("Install the package providing '{}' or update the service config at /etc/cesar/services/{}.ini", exec, name),
                ));

                let parts: Vec<&str> = exec.split_whitespace().collect();
                if !parts.is_empty() {
                    let bin_name = parts[0];
                    if let Some(fname) = Path::new(bin_name).file_name() {
                        let fname_str = fname.to_string_lossy();
                        let search_paths = ["/bin", "/sbin", "/usr/bin", "/usr/sbin", "/system/bin"];
                        let mut found = false;
                        for sp in &search_paths {
                            let full = format!("{}/{}", sp, fname_str);
                            if Path::new(&full).exists() {
                                diagnostics.push((
                                    format!("Found similar binary at {}", full),
                                    format!("Update Exec in /etc/cesar/services/{}.ini to use '{}'", name, full),
                                ));
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            diagnostics.push((
                                format!("Binary '{}' not found in standard PATH locations", fname_str),
                                "Run 'which <binary>' to locate it, or install the required package".to_string(),
                            ));
                        }
                    }
                }
            } else {
                diagnostics.push((
                    format!("Binary '{}' exists but is not executable", exec),
                    format!("Run: chmod +x {}", exec),
                ));
            }
        } else if error_msg.contains("Permission denied") {
            diagnostics.push((
                format!("Permission denied executing '{}'", exec),
                format!("Run: chmod +x {} && chown root:root {}", exec, exec),
            ));
        } else if error_msg.contains("Fork failed") {
            diagnostics.push((
                "Process table full or insufficient memory".to_string(),
                "Check system limits with 'ulimit -a' and reduce max user processes if needed".to_string(),
            ));
        } else if error_msg.contains("Exec path") {
            diagnostics.push((
                format!("Cannot execute '{}'", exec),
                "Verify the binary exists and is a valid ELF executable".to_string(),
            ));
        } else {
            diagnostics.push((
                format!("Service '{}' failed to start", name),
                format!("Check the service config at /etc/cesar/services/{}.ini and verify the Exec path", name),
            ));
        }

        if !svc.config.requires.is_empty() {
            for dep in &svc.config.requires {
                if let Some(dep_svc) = dag.services.get(dep) {
                    if matches!(dep_svc.state, ServiceState::Failed) {
                        diagnostics.push((
                            format!("Dependency '{}' has failed", dep),
                            format!("Fix dependency '{}' first, then restart '{}'", dep, name),
                        ));
                    } else if !matches!(dep_svc.state, ServiceState::Running) {
                        diagnostics.push((
                            format!("Dependency '{}' is not running (state: {})", dep, dep_svc.state),
                            format!("Start '{}' before starting '{}'", dep, name),
                        ));
                    }
                } else {
                    diagnostics.push((
                        format!("Dependency '{}' is not defined", dep),
                        format!("Create a service config for '{}' in /etc/cesar/services/ or /system/lib/cesar/services/", dep),
                    ));
                }
            }
        }
    }

    if diagnostics.is_empty() {
        diagnostics.push((
            format!("Service '{}' failed with unknown error", name),
            format!("Check /var/log/cesar.md for details and verify /etc/cesar/services/{}.ini", name),
        ));
    }

    diagnostics
}

pub fn build_error_tree(dag: &DagEngine, failed_services: &[(String, String)]) -> String {
    let mut out = String::new();
    let version = env!("CARGO_PKG_VERSION");

    out.push_str(&format!("{}\n", CESAR_BANNER));
    out.push_str(&format!(" (v{})\n", version));
    out.push_str(&format!(" ({})\n\n", "/var/log/cesar.md"));

    out.push_str(&format!(
        "[!] SYSTEM/BOOT: ({} error(s))\n",
        failed_services.len()
    ));
    out.push_str(&"─".repeat(50));
    out.push_str("\n\n");
    out.push_str("└─┬─ [System Init Base]\n");

    let boot_order = dag.get_boot_order();

    for (level_idx, level) in boot_order.iter().enumerate() {
        for (i, name) in level.iter().enumerate() {
            let is_last_in_level = i == level.len() - 1;
            let is_last_level = level_idx == boot_order.len() - 1;
            let is_last_overall = is_last_in_level && is_last_level;

            let state = dag.get_service_state(name).unwrap_or(ServiceState::Stopped);
            let state_str = match state {
                ServiceState::Running => "\x1b[32m[OK]\x1b[0m".to_string(),
                ServiceState::Failed => "\x1b[31m[FAILED]\x1b[0m".to_string(),
                ServiceState::Starting => "\x1b[33m[STARTING]\x1b[0m".to_string(),
                ServiceState::Reloading => "\x1b[33m[RELOADING]\x1b[0m".to_string(),
                ServiceState::Stopping => "\x1b[33m[STOPPING]\x1b[0m".to_string(),
                _ => format!("[{}]", state),
            };

            let connector = if is_last_overall { "  └─" } else { "  ├─" };
            let name_display = format!("{:<20}", format!("{}.ini", name));
            out.push_str(&format!("{}─► {} ──── {}\n", connector, name_display, state_str));

            if state == ServiceState::Failed
                && let Some((_, error_msg)) = failed_services.iter().find(|(n, _)| n == name) {
                    let prefix = if is_last_overall { "      " } else { "  │   " };

                    let diagnostics = diagnose_service_failure(name, error_msg, dag);

                    out.push_str(&format!("{}  └───┼───► [ERROR] ───► {}\n", prefix, error_msg));

                    for (i, (diagnosis, fix)) in diagnostics.iter().enumerate() {
                        let is_last_diag = i == diagnostics.len() - 1;
                        let branch = if is_last_diag {
                            format!("{}       └───", prefix)
                        } else {
                            format!("{}       ├───", prefix)
                        };
                        out.push_str(&format!("{}► [CTX] ───► {}\n", branch, diagnosis));
                        out.push_str(&format!("{}     │\n", prefix));
                        out.push_str(&format!("{}     └───► [FIX] ───► {}\n", prefix, fix));
                        if !is_last_diag {
                            out.push_str(&format!("{}       │\n", prefix));
                        }
                    }
                }
        }

        if level_idx < boot_order.len() - 1 {
            out.push_str("  │\n");
        }
    }

    out.push('\n');
    out
}

pub fn print_status(dag: &DagEngine) {
    let boot_order = dag.get_boot_order();

    println!("{}", CESAR_BANNER);
    println!(" Service Status Dashboard");
    println!("{}", "─".repeat(40));

    for level in &boot_order {
        for name in level {
            let state = dag.get_service_state(name).unwrap_or(ServiceState::Stopped);
            let state_str = match state {
                ServiceState::Running => "\x1b[32m[OK]\x1b[0m",
                ServiceState::Failed => "\x1b[31m[FAILED]\x1b[0m",
                ServiceState::Starting => "\x1b[33m[STARTING]\x1b[0m",
                ServiceState::Reloading => "\x1b[33m[RELOADING]\x1b[0m",
                ServiceState::Stopping => "\x1b[33m[STOPPING]\x1b[0m",
                _ => "\x1b[90m[STOPPED]\x1b[0m",
            };
            println!("  {}.ini ──── {}", name, state_str);
        }
    }
    println!();
}
