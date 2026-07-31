use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;

static FOLLOW_RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_sigint(_sig: i32) {
    FOLLOW_RUNNING.store(false, Ordering::SeqCst);
}

const CESAR_BANNER: &str = r#"
 ████████╗████████╗████████╗   ███████╗  ████████╗
███╔═════╝██╔═════╝██╔═════╝ ███╔═══███╗ ███╔═══███╗
███║      ███████╗ ████████╗ ██████████║ █████████╔╝
███║      ██╔════╝ ╚═════██║ ███╔═══███║ ███╔═══███╗
╚████████╗████████╗████████║ ███║   ███║ ███║   ███║
 ╚═══════╝╚═══════╝╚═══════╝ ╚══╝   ╚══╝ ╚══╝   ╚══╝"#;

const LOG_PATH: &str = "/var/log/cesar.md";

#[derive(Parser)]
#[command(name = "csl", version, about = "Cesar Log Viewer")]
struct Cli {

    #[arg(short = 's', long = "service")]
    service: Option<String>,


    #[arg(short = 'e', long = "errors-only")]
    errors_only: bool,


    #[arg(short = 'f', long = "follow")]
    follow: bool,


    #[arg(short = 'n', long = "lines", default_value = "50")]
    lines: usize,


    #[arg(short = 'g', long = "grep")]
    grep: Option<String>,


    #[arg(short = 't', long = "tree")]
    tree: bool,


    #[arg(short = 'T', long = "tail")]
    tail: Option<usize>,


    #[arg(short = 'c', long = "clear")]
    clear: bool,


    #[arg(short = 'S', long = "stats")]
    stats: bool,


    #[arg(short = 'm', long = "summary")]
    summary: bool,


    #[arg(short = 'j', long = "json")]
    json: bool,
}

fn read_log() -> String {
    fs::read_to_string(LOG_PATH).unwrap_or_else(|_| {
        eprintln!("csl: No log file at {}", LOG_PATH);
        process::exit(1);
    })
}

fn parse_log_sections(content: &str) -> Vec<(String, Vec<String>)> {
    let mut sections = Vec::new();
    let mut current: Option<String> = None;
    let mut lines = Vec::new();
    for line in content.lines() {
        if line.starts_with("## [") {
            if let Some(name) = current.take() {
                sections.push((name, lines.clone()));
                lines.clear();
            }
            let name = line.trim_start_matches("## [").trim_end_matches(']').to_string();
            current = Some(name);
        } else if current.is_some() && !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    if let Some(name) = current {
        sections.push((name, lines));
    }
    sections
}

fn diagnose_error(service_name: &str, error_msg: &str) -> Vec<(String, String)> {
    let mut diag = Vec::new();

    if error_msg.contains("Exec") && error_msg.contains("not found") {
        let exec_path = error_msg.split('\'').nth(1).unwrap_or_default();

        if !exec_path.is_empty() {
            if !Path::new(&exec_path).exists() {
                diag.push((
                    format!("Binary '{}' does not exist", exec_path),
                    format!("Install the package providing '{}' or update Exec in /etc/cesar/services/{}.ini", exec_path, service_name),
                ));

                let bin_name = Path::new(&exec_path).file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();

                if !bin_name.is_empty() {
                    let search_paths = ["/bin", "/sbin", "/usr/bin", "/usr/sbin", "/system/bin"];
                    for sp in &search_paths {
                        let full = format!("{}/{}", sp, bin_name);
                        if Path::new(&full).exists() {
                            diag.push((
                                format!("Found similar binary at {}", full),
                                format!("Update Exec in /etc/cesar/services/{}.ini to '{}'", service_name, full),
                            ));
                            break;
                        }
                    }
                }
            } else if let Ok(meta) = fs::metadata(exec_path)
                && meta.permissions().mode() & 0o111 == 0 {
                    diag.push((
                        format!("Binary '{}' exists but is not executable", exec_path),
                        format!("Run: chmod +x {}", exec_path),
                    ));
                }
        }
    } else if error_msg.contains("Permission denied") {
        diag.push((
            "Insufficient permissions to execute the service binary".to_string(),
            "Run as root or adjust file permissions on the binary".to_string(),
        ));
    } else if error_msg.contains("Fork failed") {
        diag.push((
            "Cannot create new process".to_string(),
            "Check system limits with 'ulimit -a' and reduce max user processes".to_string(),
        ));
    }

    if diag.is_empty() {
        diag.push((
            format!("Service '{}' encountered an error", service_name),
            format!("Check the service config at /etc/cesar/services/{}.ini", service_name),
        ));
    }

    diag
}

fn main() {
    let cli = Cli::parse();

    if cli.clear {
        let header = format!(
            "# ─── CESAR SYSTEM LOGS SUMMARY ───\nDate: {} | Host: Cudane\n\n",
            chrono::Local::now().format("%Y-%m-%d")
        );
        fs::write(LOG_PATH, &header).ok();
        println!("\x1b[32m✓\x1b[0m Log cleared");
        return;
    }

    if cli.tree {
        let content = read_log();
        let sections = parse_log_sections(&content);
        let errors: Vec<(&str, &str)> = sections
            .iter()
            .filter(|(_, lines)| lines.iter().any(|l| l.contains("CRITICAL")))
            .flat_map(|(name, lines)| {
                lines.iter()
                    .filter(|l| l.contains("CRITICAL"))
                    .map(move |line| (name.as_str(), line.as_str()))
            })
            .collect();

        println!("{}", CESAR_BANNER);
        println!(" Error Tree");
        println!("{}", "─".repeat(50));

        if errors.is_empty() {
            println!("\x1b[32m✓ No errors found.\x1b[0m");
            return;
        }

        println!("\n[!] {} ERROR(S):", errors.len());
        println!("{}", "─".repeat(50));
        println!("\n└─┬─ [Log Analysis]");

        let mut current_svc = "";
        for (i, (svc, line)) in errors.iter().enumerate() {
            let is_last = i == errors.len() - 1;
            let connector = if is_last { "  └─" } else { "  ├─" };

            if *svc != current_svc {
                current_svc = svc;
                println!("{}─► {:<20} [FAILED]", connector, format!("{}.ini", svc));
            }

            let prefix = if is_last { "      " } else { "  │   " };
            let msg = line.trim_start_matches('>').trim_start()
                .trim_start_matches('[').trim_start_matches(char::is_numeric)
                .trim_start_matches(']').trim_start();
            println!("{}  └───┼───► [ERROR] ───► {}", prefix, msg);

            let diagnostics = diagnose_error(svc, msg);
            for (j, (diagnosis, fix)) in diagnostics.iter().enumerate() {
                let is_last_diag = j == diagnostics.len() - 1;
                let branch = if is_last_diag {
                    format!("{}       └───", prefix)
                } else {
                    format!("{}       ├───", prefix)
                };
                println!("{}► [CTX] ───► {}", branch, diagnosis);
                println!("{}     │", prefix);
                println!("{}     └───► [FIX] ───► {}", prefix, fix);
                if !is_last_diag {
                    println!("{}       │", prefix);
                }
            }
        }
        println!();
        return;
    }

    if let Some(n) = cli.tail {
        let content = read_log();
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(n);
        println!("{}", CESAR_BANNER);
        println!(" Last {} lines", n);
        println!("{}", "─".repeat(40));
        for line in &lines[start..] {
            println!("{}", line);
        }
        return;
    }

    if cli.stats {
        let content = read_log();
        let sections = parse_log_sections(&content);
        println!("{}", CESAR_BANNER);
        println!(" Log Statistics");
        println!("{}", "─".repeat(40));
        for (name, lines) in &sections {
            let errors = lines.iter().filter(|l| l.contains("CRITICAL")).count();
            let warnings = lines.iter().filter(|l| l.contains("WARNING")).count();
            println!("  {:<20} {} entries ({} err, {} warn)", name, lines.len(), errors, warnings);
        }
        return;
    }

    if cli.summary {
        let content = read_log();
        let total = content.lines().count();
        let errors = content.lines().filter(|l| l.contains("CRITICAL")).count();
        let warnings = content.lines().filter(|l| l.contains("WARNING")).count();
        println!("{}", CESAR_BANNER);
        println!(" Log Summary");
        println!("{}", "─".repeat(40));
        println!("  Total:     {}", total);
        println!("  Errors:    {}", errors);
        println!("  Warnings:  {}", warnings);
        return;
    }

    if let Some(ref pattern) = cli.grep {
        let content = read_log();
        println!("{}", CESAR_BANNER);
        println!(" Searching for '{}'", pattern);
        println!("{}", "─".repeat(40));
        for (i, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&pattern.to_lowercase()) {
                println!("  L{}: {}", i + 1, line);
            }
        }
        return;
    }

    if cli.follow {
        println!("{}", CESAR_BANNER);
        println!(" Following log (Ctrl+C to stop)...");
        println!("{}", "─".repeat(40));
        let content = read_log();
        let mut last_len = content.len();
        FOLLOW_RUNNING.store(true, Ordering::SeqCst);
        unsafe {
            nix::sys::signal::signal(
                nix::sys::signal::Signal::SIGINT,
                nix::sys::signal::SigHandler::Handler(handle_sigint),
            ).ok();
        }
        while FOLLOW_RUNNING.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let new_content = read_log();
            if new_content.len() > last_len {
                print!("{}", &new_content[last_len..]);
                std::io::stdout().flush().ok();
                last_len = new_content.len();
            }
        }
        println!("\nStopped.");
    }

    let content = read_log();
    let sections = parse_log_sections(&content);

    println!("{}", CESAR_BANNER);
    println!(" System Log");
    println!("{}", "─".repeat(40));

    for (name, lines) in &sections {
        if let Some(ref filter) = cli.service
            && name != filter { continue; }

        let filtered: Vec<&String> = if cli.errors_only {
            lines.iter().filter(|l| l.contains("CRITICAL") || l.contains("ERROR")).collect()
        } else {
            lines.iter().collect()
        };

        if !filtered.is_empty() {
            println!("\n## [{}]", name);
            for line in &filtered {
                println!("  {}", line);
            }
        }
    }
}