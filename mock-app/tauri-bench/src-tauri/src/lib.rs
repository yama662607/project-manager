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
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};

// MARK: - Config

#[derive(Clone, Serialize, Deserialize)]
struct AppConfig {
    projects: Vec<Project>,
    #[serde(default)]
    shortcut: ShortcutConfig,
}

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
    logger: BenchLogger,
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
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let mut file = File::create(&tmp_path).map_err(|e| format!("create tmp: {e}"))?;
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
    json!({ "projects": *projects, "shortcut": ShortcutConfig::default() })
}

#[tauri::command]
fn save_config(app: AppHandle, config: Value) -> Result<String, String> {
    let has_shortcut = config.get("shortcut").is_some();
    let mut parsed: AppConfig =
        serde_json::from_value(config).map_err(|e| format!("invalid config: {e}"))?;
    if !has_shortcut {
        parsed.shortcut = load_config().shortcut;
    }

    save_config_file(&parsed)?;

    let state = app.state::<AppState>();
    *state.projects.lock().unwrap() = parsed.projects.clone();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("window.__PROJECT_LAUNCHER_RELOAD?.();");
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
    app: AppHandle,
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
            json!({ "target": "AppKitBench", "query": query.unwrap_or_default() }),
        );
        let result = Command::new("/usr/bin/open")
            .args(["-a", "/Applications/AppKitBench.app"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match result {
            Ok(_) => state.logger.log(
                "debug_switch_dispatched",
                cycle_id.as_deref(),
                json!({ "target": "AppKitBench" }),
            ),
            Err(error) => state.logger.log(
                "debug_switch_failed",
                cycle_id.as_deref(),
                json!({ "target": "AppKitBench", "error": error.to_string() }),
            ),
        }
        app.exit(0);
        return;
    }
    let paths = open_paths
        .filter(|paths| !paths.is_empty())
        .or_else(|| {
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
    let _ = WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
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
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
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
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            return;
        }

        let cycle_id = format!("{source}-{}", now_id());
        let state = app.state::<AppState>();
        state.logger.log(
            "hotkey_received",
            Some(&cycle_id),
            json!({ "source": source }),
        );
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
    let logger = BenchLogger::new();
    logger.log(
        "app_ready",
        None,
        json!({ "project_count": config.projects.len(), "source": "backend" }),
    );

    tauri::Builder::default()
        .manage(AppState {
            projects: Mutex::new(config.projects),
            logger,
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed
                        && shortcut.matches(Modifiers::CONTROL, Code::KeyM)
                    {
                        show_palette(app, "hotkey");
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
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            app.global_shortcut().register("Control+KeyM")?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.center();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
