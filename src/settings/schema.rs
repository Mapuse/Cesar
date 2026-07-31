use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_level() -> String { "info".into() }
fn default_log_file() -> String { "/var/log/cesar.md".into() }
fn default_event_socket() -> String { "/run/cesar/event.sock".into() }
fn default_interval() -> u64 { 1000 }
fn default_scrollback() -> u64 { 5000 }
fn default_color_depth() -> String { "truecolor".into() }
fn default_keybindings() -> String { "default".into() }
fn default_border_style() -> String { "rounded".into() }
fn default_animation() -> String { "normal".into() }
fn default_cursor_style() -> String { "block".into() }
fn default_link_underline() -> String { "hover".into() }
fn default_search_highlight_color() -> String { "yellow".into() }
fn default_bell() -> String { "visual".into() }
fn default_font_size() -> u32 { 12 }
fn default_empty() -> String { String::new() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub display: DisplayConfig,
    pub python: PythonConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub level: String,
    pub log_file: String,
    pub event_socket: String,
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
    pub animation: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            log_file: default_log_file(),
            event_socket: default_event_socket(),
            interval: default_interval(),
            scrollback: default_scrollback(),
            enable_mouse: default_true(),
            enable_ansi: default_true(),
            color_depth: default_color_depth(),
            status_bar: default_true(),
            keybindings: default_keybindings(),
            border_style: default_border_style(),
            show_help: default_true(),
            show_status: default_true(),
            show_uptime: default_true(),
            show_services: default_true(),
            show_resources: default_true(),
            show_logs: default_true(),
            compact: default_false(),
            vim: default_false(),
            notifications: default_true(),
            notification_timeout: 5000,
            animation: default_true(),
            animation: default_animation(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
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
    pub theme: String,
    pub tui: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            line_height: 1.0,
            letter_spacing: 0.0,
            padding: 2,
            margin: 1,
            scroll_offset: 0,
            wrap: default_true(),
            tab_width: 4,
            cursor_style: default_cursor_style(),
            cursor_blink: default_true(),
            selection_clipboard: default_true(),
            link_underline: default_link_underline(),
            search_case_sensitive: default_false(),
            search_regex: default_false(),
            search_highlight_color: default_search_highlight_color(),
            bell: default_bell(),
            theme: default_empty(),
            tui: default_empty(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PythonConfig {
    pub enabled: bool,
    pub theme: String,
    pub tui: String,
    pub plugins: Vec<String>,
    pub fallback_on_error: bool,
    pub venv_path: String,
    pub tui_mode: bool,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            theme: default_empty(),
            tui: default_empty(),
            plugins: vec![],
            fallback_on_error: default_true(),
            venv_path: default_empty(),
            tui_mode: default_false(),
        }
    }
}
