use std::sync::OnceLock;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;

use crate::service::{RestartPolicy, ServiceConfig};

const SYSTEM_SERVICE_DIR: &str = "/system/lib/cesar/services";
const USER_SERVICE_DIR: &str = "/etc/cesar/services";
const ENABLED_SERVICE_DIR: &str = "/etc/cesar/enabled";
const CESAR_CONFIG_PATH: &str = "/etc/cesar/cesar.ini";

// ─── Cesar Config ───────────────────────────────────────────────────────────

static CESAR_CONFIG: OnceLock<CesarConfig> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct CesarConfig {
    pub level: String,
    pub log_file: String,
    pub plugin_enabled: bool,
    pub plugin_timeout: u64,
    pub event_socket: String,
    pub tui_enabled: bool,
    pub theme: String,
    pub default_tui: String,
    pub interval: u64,
    pub scrollback: u64,
    pub enable_mouse: bool,
    pub enable_ansi: bool,
    pub color_depth: String,
    pub status_bar: bool,
    pub keybindings: String,
    pub border_style: String,
    pub show_help: bool,
    pub show_status: bool,
    pub show_uptime: bool,
    pub show_services: bool,
    pub show_resources: bool,
    pub show_logs: bool,
    pub compact: bool,
    pub vim: bool,
    pub notifications: bool,
    pub notification_timeout: u64,
    pub animation: bool,
    pub animation_speed: String,
    pub font_size: u32,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub padding: u32,
    pub margin: u32,
    pub scroll_offset: u64,
    pub wrap: bool,
    pub tab_width: u32,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub selection_clipboard: bool,
    pub link_underline: String,
    pub search_case_sensitive: bool,
    pub search_regex: bool,
    pub search_highlight_color: String,
    pub bell: String,
}

impl Default for CesarConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            log_file: "/var/log/cesar.md".into(),
            plugin_enabled: true,
            plugin_timeout: 30,
            event_socket: "/run/cesar/event.sock".into(),
            tui_enabled: true,
            theme: "default".into(),
            default_tui: "default".into(),
            interval: 1000,
            scrollback: 5000,
            enable_mouse: true,
            enable_ansi: true,
            color_depth: "truecolor".into(),
            status_bar: true,
            keybindings: "default".into(),
            border_style: "rounded".into(),
            show_help: true,
            show_status: true,
            show_uptime: true,
            show_services: true,
            show_resources: true,
            show_logs: true,
            compact: false,
            vim: false,
            notifications: true,
            notification_timeout: 5000,
            animation: true,
            animation_speed: "normal".into(),
            font_size: 12,
            line_height: 1.0,
            letter_spacing: 0.0,
            padding: 2,
            margin: 1,
            scroll_offset: 0,
            wrap: true,
            tab_width: 4,
            cursor_style: "block".into(),
            cursor_blink: true,
            selection_clipboard: true,
            link_underline: "hover".into(),
            search_case_sensitive: false,
            search_regex: false,
            search_highlight_color: "yellow".into(),
            bell: "visual".into(),
        }
    }
}

fn load_cesar_config() -> io::Result<CesarConfig> {
    let path = Path::new(CESAR_CONFIG_PATH);
    if !path.exists() {
        return Ok(CesarConfig::default());
    }
    let content = fs::read_to_string(path)?;
    Ok(parse_cesar_ini(&content))
}

fn parse_cesar_ini(content: &str) -> CesarConfig {
    let mut cfg = CesarConfig::default();
    let mut section = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len()-1].to_lowercase();
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match section.as_str() {
                "cesar" => match key {
                    "level" => cfg.level = value.into(),
                    "log_file" => cfg.log_file = value.into(),
                    _ => {}
                },
                "plugin" => match key {
                    "enabled" => cfg.plugin_enabled = parse_bool(value),
                    "timeout" => cfg.plugin_timeout = value.parse().unwrap_or(30),
                    "event_socket" => cfg.event_socket = value.into(),
                    _ => {}
                },
                "tui" => match key {
                    "enabled" => cfg.tui_enabled = parse_bool(value),
                    "theme" => cfg.theme = value.into(),
                    "default_tui" => cfg.default_tui = value.into(),
                    "interval" => cfg.interval = value.parse().unwrap_or(1000),
                    "scrollback" => cfg.scrollback = value.parse().unwrap_or(5000),
                    "enable_mouse" => cfg.enable_mouse = parse_bool(value),
                    "enable_ansi" => cfg.enable_ansi = parse_bool(value),
                    "color_depth" => cfg.color_depth = value.into(),
                    "status_bar" => cfg.status_bar = parse_bool(value),
                    "keybindings" => cfg.keybindings = value.into(),
                    "border_style" => cfg.border_style = value.into(),
                    "show_help" => cfg.show_help = parse_bool(value),
                    "show_status" => cfg.show_status = parse_bool(value),
                    "show_uptime" => cfg.show_uptime = parse_bool(value),
                    "show_services" => cfg.show_services = parse_bool(value),
                    "show_resources" => cfg.show_resources = parse_bool(value),
                    "show_logs" => cfg.show_logs = parse_bool(value),
                    "compact" => cfg.compact = parse_bool(value),
                    "vim" => cfg.vim = parse_bool(value),
                    "notifications" => cfg.notifications = parse_bool(value),
                    "notification_timeout" => cfg.notification_timeout = value.parse().unwrap_or(5000),
                    "animation" => cfg.animation = parse_bool(value),
                    "animation_speed" => cfg.animation_speed = value.into(),
                    "font_size" => cfg.font_size = value.parse().unwrap_or(12),
                    "line_height" => cfg.line_height = value.parse().unwrap_or(1.0),
                    "letter_spacing" => cfg.letter_spacing = value.parse().unwrap_or(0.0),
                    "padding" => cfg.padding = value.parse().unwrap_or(2),
                    "margin" => cfg.margin = value.parse().unwrap_or(1),
                    "scroll_offset" => cfg.scroll_offset = value.parse().unwrap_or(0),
                    "wrap" => cfg.wrap = parse_bool(value),
                    "tab_width" => cfg.tab_width = value.parse().unwrap_or(4),
                    "cursor_style" => cfg.cursor_style = value.into(),
                    "cursor_blink" => cfg.cursor_blink = parse_bool(value),
                    "selection_clipboard" => cfg.selection_clipboard = parse_bool(value),
                    "link_underline" => cfg.link_underline = value.into(),
                    "search_case_sensitive" => cfg.search_case_sensitive = parse_bool(value),
                    "search_regex" => cfg.search_regex = parse_bool(value),
                    "search_highlight_color" => cfg.search_highlight_color = value.into(),
                    "bell" => cfg.bell = value.into(),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    cfg
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "true" | "yes" | "1" | "on")
}

pub fn get_config() -> &'static CesarConfig {
    CESAR_CONFIG.get_or_init(|| {
        load_cesar_config().unwrap_or_default()
    })
}

pub fn parse_cesar_config(file_path: &str) -> io::Result<ServiceConfig> {
    let path = Path::new(file_path);
    let content = fs::read_to_string(path)?;
    parse_config_content(&content, file_path)
}

fn parse_config_content(content: &str, source: &str) -> io::Result<ServiceConfig> {
    let mut name = String::new();
    let mut exec = String::new();
    let mut requires = Vec::new();
    let mut restart = String::new();
    let mut socket: Option<String> = None;
    let mut description: Option<String> = None;
    let mut environment: Vec<(String, String)> = Vec::new();
    let mut working_directory: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "Name" => name = value.to_string(),
                "Exec" => exec = value.to_string(),
                "Requires" => {
                    requires = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "Restart" => restart = value.to_string(),
                "Socket" => socket = Some(value.to_string()),
                "Description" => description = Some(value.to_string()),
                "Environment" => {
                    for part in value.split_whitespace() {
                        if let Some((k, v)) = part.split_once('=') {
                            environment.push((k.to_string(), v.to_string()));
                        }
                    }
                }
                "WorkingDirectory" => working_directory = Some(value.to_string()),
                _ => {
                    eprintln!("[Warning] :: Unknown key '{}' in {} ignored.", key, source);
                }
            }
        }
    }

    if name.is_empty() || exec.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Missing required Name or Exec in {}", source),
        ));
    }

    Ok(ServiceConfig {
        name,
        exec,
        requires,
        restart: RestartPolicy::parse(&restart),
        socket,
        description,
        environment,
        working_directory,
    })
}

pub fn load_all_services() -> Vec<ServiceConfig> {
    let mut services_by_name: HashMap<String, ServiceConfig> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    if let Ok(entries) = fs::read_dir(SYSTEM_SERVICE_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                match parse_cesar_config(path.to_str().unwrap_or_default()) {
                    Ok(cfg) => {
                        let key = cfg.name.clone();
                        order.push(key.clone());
                        services_by_name.insert(key, cfg);
                    }
                    Err(e) => eprintln!("[Error] :: Failed to parse {:?}: {}", path, e),
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(USER_SERVICE_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                match parse_cesar_config(path.to_str().unwrap_or_default()) {
                    Ok(cfg) => {
                        let key = cfg.name.clone();
                        if !services_by_name.contains_key(&key) {
                            order.push(key.clone());
                        }
                        services_by_name.insert(key, cfg);
                    }
                    Err(e) => eprintln!("[Error] :: Failed to parse {:?}: {}", path, e),
                }
            }
        }
    }

    let enabled: Option<HashSet<String>> = fs::read_dir(ENABLED_SERVICE_DIR)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    file_name.strip_suffix(".ini").map(|s| s.to_string())
                })
                .collect()
        });

    order
        .into_iter()
        .filter(|k| match &enabled {
            Some(set) => set.contains(k),
            None => true,
        })
        .filter_map(|k| services_by_name.remove(&k))
        .collect()
}
