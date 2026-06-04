use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Instant, SystemTime},
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// MARK: - Config

#[derive(Clone, Serialize, Deserialize)]
struct ShortcutConfig {
    modifiers: Vec<String>,
    key: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            modifiers: vec!["control".into()],
            key: "m".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct AppConfig {
    projects: Vec<Project>,
    #[serde(default)]
    shortcut: ShortcutConfig,
}

// MARK: - Project

#[derive(Clone, Serialize, Deserialize)]
struct Project {
    id: String,
    name: String,
    path: String,
    #[serde(rename = "openPaths", default)]
    open_paths: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    tags: Vec<String>,
    language: String,
    #[serde(rename = "lastOpenedAt", default)]
    last_opened_at: String,
}

// MARK: - BenchLogger

struct BenchLogger {
    start: Instant,
    file: Mutex<File>,
}

impl BenchLogger {
    fn new() -> Self {
        let dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Logs/ProjectLauncherBench");
        let _ = fs::create_dir_all(&dir);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("tauri-{}.jsonl", std::process::id())))
            .expect("open metrics log");
        Self {
            start: Instant::now(),
            file: Mutex::new(file),
        }
    }

    fn mono_ns(&self) -> u128 {
        self.start.elapsed().as_nanos()
    }

    fn log(&self, event: &str, cycle_id: Option<&str>, fields: Value) {
        let mut payload = match fields {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        payload.insert("app".into(), json!("tauri"));
        payload.insert("event".into(), json!(event));
        payload.insert("mono_ns".into(), json!(self.mono_ns()));
        payload.insert(
            "wall_ms".into(),
            json!(SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default()),
        );
        if let Some(cycle_id) = cycle_id {
            payload.insert("cycle_id".into(), json!(cycle_id));
        }

        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{}", Value::Object(payload));
        }
    }
}

// MARK: - AppState

struct AppState {
    projects: Mutex<Vec<Project>>,
    shortcut_string: Mutex<String>,
    logger: BenchLogger,
    palette_visible: Mutex<bool>,
}

// MARK: - Shortcut format conversion

fn format_tauri_shortcut(modifiers: &[String], key: &str) -> String {
    let mod_parts: Vec<String> = modifiers
        .iter()
        .map(|m| match m.to_lowercase().as_str() {
            "control" | "ctrl" => "Control".into(),
            "option" | "alt" => "Alt".into(),
            "command" | "cmd" | "super" => "Super".into(),
            "shift" => "Shift".into(),
            other => other.to_string(),
        })
        .collect();

    let key_part = match key.to_lowercase().as_str() {
        " " | "space" => "Space".into(),
        c if c.len() == 1 && c.chars().next().unwrap().is_ascii_alphabetic() => {
            format!("Key{}", c.to_uppercase())
        }
        c if c.len() == 1 && c.chars().next().unwrap().is_ascii_digit() => {
            format!("Digit{}", c)
        }
        other => other.to_string(),
    };

    if mod_parts.is_empty() {
        key_part
    } else {
        format!("{}+{key_part}", mod_parts.join("+"))
    }
}

fn parse_tauri_shortcut(s: &str) -> (Vec<String>, String) {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return (vec!["control".into()], "m".into());
    }

    let key_part = parts.last().unwrap().trim();
    let modifiers: Vec<String> = parts[..parts.len() - 1]
        .iter()
        .map(|m| match m.trim() {
            "Control" => "control".into(),
            "Alt" => "option".into(),
            "Super" => "command".into(),
            "Shift" => "shift".into(),
            other => other.to_lowercase(),
        })
        .collect();

    let key = if let Some(rest) = key_part.strip_prefix("Key") {
        rest.to_lowercase()
    } else if let Some(rest) = key_part.strip_prefix("Digit") {
        rest.to_lowercase()
    } else if key_part == "Space" {
        "space".into()
    } else {
        key_part.to_lowercase()
    };

    (modifiers, key)
}

// MARK: - Config load/save

fn load_config() -> AppConfig {
    if let Some(home) = dirs::home_dir() {
        let path = home.join(".project-manager.json");
        if let Ok(data) = fs::read(&path) {
            if let Ok(config) = serde_json::from_slice::<AppConfig>(&data) {
                return config;
            }
        }
    }
    AppConfig {
        projects: load_fixture(),
        shortcut: ShortcutConfig::default(),
    }
}

fn save_config_file(config: &AppConfig) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let config_path = home.join(".project-manager.json");
    let tmp_path = home.join(".project-manager.json.tmp");
    let data =
        serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let mut file =
        File::create(&tmp_path).map_err(|e| format!("create tmp: {e}"))?;
    file.write_all(data.as_bytes())
        .map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp_path, &config_path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

// MARK: - Tauri commands

#[tauri::command]
fn load_projects(state: tauri::State<'_, AppState>) -> Vec<Project> {
    state.projects.lock().unwrap().clone()
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> Value {
    let projects = state.projects.lock().unwrap();
    let shortcut_str = state.shortcut_string.lock().unwrap();
    let (modifiers, key) = parse_tauri_shortcut(&shortcut_str);
    json!({
        "projects": *projects,
        "shortcut": { "modifiers": modifiers, "key": key }
    })
}

#[tauri::command]
fn save_config(app: AppHandle, config: Value) -> Result<String, String> {
    let parsed: AppConfig =
        serde_json::from_value(config).map_err(|e| format!("invalid config: {e}"))?;

    save_config_file(&parsed)?;

    // Update app state
    let shortcut_str = format_tauri_shortcut(&parsed.shortcut.modifiers, &parsed.shortcut.key);
    {
        let state = app.state::<AppState>();
        *state.projects.lock().unwrap() = parsed.projects.clone();

        let old_shortcut = state.shortcut_string.lock().unwrap().clone();
        let gs = app.global_shortcut();
        if let Err(e) = gs.unregister(&old_shortcut as &str) {
            return Err(format!("unregister old shortcut: {e}"));
        }
        if let Err(e) = gs.register(&shortcut_str as &str) {
            return Err(format!("register new shortcut: {e}"));
        }
        *state.shortcut_string.lock().unwrap() = shortcut_str;
    }

    Ok("saved".into())
}

#[tauri::command]
fn log_event(
    state: tauri::State<'_, AppState>,
    event: String,
    cycle_id: Option<String>,
    fields: Option<Value>,
) {
    state.logger.log(
        &event,
        cycle_id.as_deref(),
        fields.unwrap_or_else(|| json!({})),
    );
}

#[tauri::command]
fn log_metric(
    state: tauri::State<'_, AppState>,
    event: String,
    cycle_id: Option<String>,
    metric: String,
    duration_ms: f64,
    query: String,
    result_count: usize,
) {
    state.logger.log(
        &event,
        cycle_id.as_deref(),
        json!({
            "metric": metric,
            "duration_ms": duration_ms,
            "query": query,
            "result_count": result_count
        }),
    );
}

#[tauri::command]
fn open_project(
    state: tauri::State<'_, AppState>,
    cycle_id: Option<String>,
    path: String,
    open_paths: Option<Vec<String>>,
    project_id: String,
    scenario: Option<String>,
    query: Option<String>,
    selected_index: Option<usize>,
) {
    state
        .logger
        .log("open_requested", cycle_id.as_deref(), json!({}));
    if project_id == "debug-switch-to-appkit" {
        state.logger.log(
            "debug_switch_requested",
            cycle_id.as_deref(),
            json!({ "target": "AppKitBench", "query": query }),
        );
        let _ = Command::new("/usr/bin/open")
            .args(["-a", "/Applications/AppKitBench.app"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        state.logger.log(
            "debug_switch_dispatched",
            cycle_id.as_deref(),
            json!({ "target": "AppKitBench" }),
        );
        std::process::exit(0);
    }
    let paths = open_paths
        .filter(|paths| !paths.is_empty())
        .or_else(|| {
            // Try to load openPaths from config
            let projects = state.projects.lock().ok()?;
            let project = projects.iter().find(|p| p.id == project_id)?;
            if !project.open_paths.is_empty() {
                Some(project.open_paths.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| vec![path]);
    let _ = zed_command()
        .args(paths)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    state.logger.log(
        "open_dispatched",
        cycle_id.as_deref(),
        json!({
            "project_id": project_id,
            "scenario": scenario.unwrap_or_default(),
            "query": query.unwrap_or_default(),
            "selected_index": selected_index.unwrap_or_default()
        }),
    );
}

#[tauri::command]
fn close_palette_command(app: AppHandle) {
    close_palette(&app);
}

#[tauri::command]
fn open_settings_window(app: AppHandle) {
    let existing = app.get_webview_window("settings");
    if let Some(win) = existing {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    use tauri::WebviewWindowBuilder;
    let _ = WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("settings.html".into()))
        .title("Settings")
        .inner_size(640.0, 520.0)
        .resizable(false)
        .center()
        .build();
}

#[tauri::command]
fn browse_folder() -> Option<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(r#"POSIX path of (choose folder with prompt "Select a project folder")"#)
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { None } else { Some(path) }
    } else {
        None
    }
}

#[tauri::command]
fn close_settings_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.close();
    }
}

// MARK: - Helpers

fn zed_command() -> Command {
    if let Some(path) = resolve_zed_command() {
        Command::new(path)
    } else {
        let mut command = Command::new("/usr/bin/env");
        command.arg("zed");
        command
    }
}

fn resolve_zed_command() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join("zed");
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    ["/usr/local/bin/zed", "/opt/homebrew/bin/zed"]
        .into_iter()
        .map(PathBuf::from)
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn show_palette<R: Runtime>(app: &AppHandle<R>, source: &str) {
    let state = app.state::<AppState>();
    let mut palette_visible = state.palette_visible.lock().unwrap();

    // Toggle: if already visible, hide and return
    if *palette_visible {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        *palette_visible = false;
        return;
    }

    // Show
    *palette_visible = true;
    drop(palette_visible);

    let cycle_id = format!("{source}-{}", now_id());
    state.logger.log(
        "hotkey_received",
        Some(&cycle_id),
        json!({ "source": source }),
    );

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
        let payload = json!({ "cycle_id": cycle_id, "source": source });
        let _ = window.eval(format!("window.__PROJECT_LAUNCHER_SHOW?.({payload});"));
    }
}

fn close_palette<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    if let Some(state) = app.try_state::<AppState>() {
        *state.palette_visible.lock().unwrap() = false;
    }
}

fn run_benchmark<R: Runtime>(app: AppHandle<R>) {
    let queries = ["a", "pr", "api", "web", "manager", "ios", "zed"];
    for index in 0..100 {
        let cycle_id = format!("benchmark-{}", now_id());
        let query = queries[index % queries.len()];
        {
            let state = app.state::<AppState>();
            state.logger.log(
                "hotkey_received",
                Some(&cycle_id),
                json!({ "source": "benchmark" }),
            );
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.center();
            let _ = window.show();
            let _ = window.set_focus();
            let payload = json!({ "cycle_id": cycle_id, "query": query });
            let _ = window.eval(format!("window.__PROJECT_LAUNCHER_BENCHMARK?.({payload});"));
        }
        std::thread::sleep(std::time::Duration::from_millis(14));
    }
    let state = app.state::<AppState>();
    state
        .logger
        .log("benchmark_cycle_completed", None, json!({ "count": 100 }));
}

fn now_id() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn load_fixture() -> Vec<Project> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                candidates.push(contents_dir.join("Resources/projects.json"));
            }
        }
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("../shared/projects.json"));
        candidates.push(current.join("shared/projects.json"));
    }

    for path in candidates {
        if let Ok(data) = fs::read(&path) {
            if let Ok(projects) = serde_json::from_slice::<Vec<Project>>(&data) {
                return projects;
            }
        }
    }
    Vec::new()
}

pub fn run() {
    let config = load_config();
    let shortcut_str = format_tauri_shortcut(&config.shortcut.modifiers, &config.shortcut.key);
    let logger = BenchLogger::new();
    logger.log(
        "app_ready",
        None,
        json!({ "project_count": config.projects.len(), "source": "backend" }),
    );

    let registered_shortcut = shortcut_str.clone();
    tauri::Builder::default()
        .manage(AppState {
            projects: Mutex::new(config.projects),
            shortcut_string: Mutex::new(shortcut_str),
            logger,
            palette_visible: Mutex::new(false),
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let state = app.state::<AppState>();
                        let current = state.shortcut_string.lock().unwrap();
                        if shortcut.to_string() == *current {
                            drop(current);
                            show_palette(app, "hotkey");
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            load_projects,
            get_config,
            save_config,
            log_event,
            log_metric,
            open_project,
            close_palette_command,
            open_settings_window,
            close_settings_window,
            browse_folder
        ])
        .setup(move |app| {
            app.global_shortcut().register(registered_shortcut.as_str())?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.center();
            }

            // --- Tray icon ---
            let tray_show = MenuItemBuilder::with_id("tray_show", "Show Palette").build(app)?;
            let tray_settings = MenuItemBuilder::with_id("tray_settings", "Settings\u{2026}").build(app)?;
            let tray_benchmark = MenuItemBuilder::with_id("tray_benchmark", "Run Benchmark").build(app)?;
            let tray_quit = MenuItemBuilder::with_id("tray_quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&tray_show, &tray_settings, &tray_benchmark, &tray_quit])
                .build()?;
            TrayIconBuilder::new()
                .tooltip("TauriBench")
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "tray_show" => show_palette(app, "menu"),
                    "tray_settings" => {
                        let _ = open_settings_window(app.clone());
                    }
                    "tray_benchmark" => run_benchmark(app.clone()),
                    "tray_quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_palette(&tray.app_handle(), "tray");
                    }
                })
                .build(app)?;

            // Show palette on first launch
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                show_palette(&handle, "launch");
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
