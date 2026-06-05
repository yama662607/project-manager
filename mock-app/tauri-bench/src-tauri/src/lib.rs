use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
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

const APP_NAME: &str = "tauri";
const MAX_RESULTS: usize = 50;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
struct AppConfig {
    projects: Vec<Project>,
    #[serde(default)]
    shortcut: ShortcutConfig,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    id: String,
    name: String,
    path: String,
    #[serde(default)]
    open_paths: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default)]
    last_opened_at: String,
}

#[derive(Clone)]
struct IndexedProject {
    project: Project,
    id: String,
    name: String,
    path: String,
    aliases: String,
    alias_list: Vec<String>,
    tags: String,
}

#[derive(Clone)]
struct SearchResult {
    project: Project,
    score: i32,
    matched_alias: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewItem {
    id: String,
    name: String,
    path: String,
    aliases: Vec<String>,
    language: String,
    is_debug: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewState {
    visible: bool,
    query: String,
    selected_index: i32,
    total_matches: usize,
    results: Vec<ViewItem>,
    footer: String,
    cycle_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyInput {
    key: String,
}

struct SearchOutcome {
    duration_ms: f64,
    alias_hit: String,
    scenario: String,
}

struct LauncherData {
    projects: Vec<Project>,
    indexed: Vec<IndexedProject>,
    alias_lookup: HashMap<String, usize>,
    query: String,
    results: Vec<SearchResult>,
    selected_index: i32,
    total_matches: usize,
    active_cycle_id: Option<String>,
    active_scenario: String,
    visible: bool,
}

struct BenchLogger {
    start: Instant,
    file: Mutex<File>,
}

struct AppState {
    inner: Mutex<LauncherData>,
    logger: BenchLogger,
}

fn default_language() -> String {
    "Project".into()
}

fn debug_switch_project() -> Project {
    Project {
        id: "debug-switch-to-appkit".into(),
        name: "Switch to AppKitBench".into(),
        path: "/Applications/AppKitBench.app".into(),
        open_paths: Vec::new(),
        aliases: vec!["-".into()],
        tags: vec!["debug".into(), "switch".into()],
        language: "Action".into(),
        last_opened_at: String::new(),
    }
}

fn debug_switch_enabled() -> bool {
    std::env::var("PROJECT_LAUNCHER_DEBUG_SWITCH")
        .ok()
        .as_deref()
        == Some("1")
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
        payload.insert("app".into(), json!(APP_NAME));
        payload.insert("event".into(), json!(event));
        payload.insert("mono_ns".into(), json!(self.mono_ns()));
        payload.insert(
            "wall_ms".into(),
            json!(SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
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

impl LauncherData {
    fn new(projects: Vec<Project>) -> Self {
        let (indexed, alias_lookup) = build_index(&projects);
        let mut data = Self {
            projects,
            indexed,
            alias_lookup,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            total_matches: 0,
            active_cycle_id: None,
            active_scenario: String::new(),
            visible: false,
        };
        let _ = run_search(&mut data);
        data
    }

    fn reload_projects(&mut self, projects: Vec<Project>) {
        self.projects = projects;
        let (indexed, alias_lookup) = build_index(&self.projects);
        self.indexed = indexed;
        self.alias_lookup = alias_lookup;
        let _ = run_search(self);
    }
}

fn build_index(projects: &[Project]) -> (Vec<IndexedProject>, HashMap<String, usize>) {
    let all_projects = if debug_switch_enabled() {
        std::iter::once(debug_switch_project())
            .chain(projects.iter().cloned())
            .collect::<Vec<_>>()
    } else {
        projects.to_vec()
    };
    let indexed = all_projects
        .into_iter()
        .map(|project| {
            let alias_list = project
                .aliases
                .iter()
                .map(|alias| alias.to_lowercase())
                .collect::<Vec<_>>();
            IndexedProject {
                id: project.id.to_lowercase(),
                name: project.name.to_lowercase(),
                path: project.path.to_lowercase(),
                aliases: alias_list.join(" "),
                alias_list,
                tags: project.tags.join(" ").to_lowercase(),
                project,
            }
        })
        .collect::<Vec<_>>();

    let mut alias_lookup = HashMap::new();
    for (index, item) in indexed.iter().enumerate() {
        for alias in &item.alias_list {
            alias_lookup.entry(alias.clone()).or_insert(index);
        }
    }
    (indexed, alias_lookup)
}

fn run_search(data: &mut LauncherData) -> SearchOutcome {
    let start = Instant::now();
    let normalized_query = data.query.to_lowercase();
    let tokens = normalized_query
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut alias_hit = String::new();

    if let Some(index) = data.alias_lookup.get(normalized_query.trim()).copied() {
        let project = data.indexed[index].project.clone();
        alias_hit = normalized_query.trim().into();
        data.total_matches = 1;
        data.results = vec![SearchResult {
            project,
            score: 10_000,
            matched_alias: Some(alias_hit.clone()),
        }];
    } else if tokens.is_empty() {
        data.total_matches = data.indexed.len();
        data.results = data
            .indexed
            .iter()
            .take(MAX_RESULTS)
            .map(|item| SearchResult {
                project: item.project.clone(),
                score: 0,
                matched_alias: None,
            })
            .collect();
    } else {
        let mut matches = Vec::new();
        for item in &data.indexed {
            let mut total = 0;
            let mut matched = true;
            for token in &tokens {
                let score = score_token(token, item);
                if score == 0 {
                    matched = false;
                    break;
                }
                total += score;
            }
            if matched {
                matches.push(SearchResult {
                    project: item.project.clone(),
                    score: total,
                    matched_alias: None,
                });
            }
        }
        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.project.name.cmp(&b.project.name))
        });
        data.total_matches = matches.len();
        data.results = matches.into_iter().take(MAX_RESULTS).collect();
    }

    data.selected_index = if data.results.is_empty() { -1 } else { 0 };
    data.active_scenario = scenario_name(&data.query, &alias_hit);

    SearchOutcome {
        duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        alias_hit,
        scenario: data.active_scenario.clone(),
    }
}

fn score_token(token: &str, item: &IndexedProject) -> i32 {
    if item.alias_list.iter().any(|alias| alias == token) {
        return 5_000;
    }
    if item.id == token {
        return 1_400;
    }
    if item.name.starts_with(token) {
        return 1_200 - item.name.len().min(300) as i32;
    }
    if token.len() == 1 && word_has_prefix(token, &item.name) {
        return 600;
    }
    if token.len() >= 3 && item.aliases.contains(token) {
        return 1_100;
    }
    if token.len() >= 2 && word_has_prefix(token, &item.name) {
        return 1_050 - item.name.len().min(280) as i32;
    }
    if token.len() >= 3 && item.id.contains(token) {
        return 1_000;
    }
    if token.len() >= 3 && item.name.contains(token) {
        return 900 - item.name.len().min(250) as i32;
    }
    if token.len() >= 3 && item.tags.contains(token) {
        return 700;
    }
    if token.len() >= 2 && item.aliases.contains(token) {
        return 680;
    }
    if token.len() >= 2 && item.id.contains(token) {
        return 660;
    }
    if token.len() >= 2 && item.name.contains(token) {
        return 620 - item.name.len().min(250) as i32;
    }
    if token.len() >= 2 && item.tags.contains(token) {
        return 580;
    }
    if token.len() >= 3 && item.path.contains(token) {
        return 450;
    }
    if token.chars().any(|ch| ch.is_ascii_digit()) {
        return 0;
    }
    if token.len() >= 3 && fuzzy_contains(token, &item.name) {
        return 250;
    }
    if token.len() >= 3 && fuzzy_contains(token, &item.path) {
        return 120;
    }
    0
}

fn fuzzy_contains(token: &str, candidate: &str) -> bool {
    let mut index = 0;
    for ch in token.chars() {
        if let Some(found) = candidate[index..].find(ch) {
            index += found + ch.len_utf8();
        } else {
            return false;
        }
    }
    true
}

fn word_has_prefix(token: &str, candidate: &str) -> bool {
    candidate
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|word| word.starts_with(token))
}

fn scenario_name(query: &str, alias_hit: &str) -> String {
    if alias_hit == "a" {
        "alias".into()
    } else if query.eq_ignore_ascii_case("pr") {
        "narrowing".into()
    } else {
        String::new()
    }
}

fn view_state(data: &LauncherData) -> ViewState {
    let footer = if let Some(result) = data.results.first() {
        if let Some(alias) = &result.matched_alias {
            format!("alias {alias} -> {}", result.project.name)
        } else {
            format!(
                "showing {} of {} matches",
                data.results.len(),
                data.total_matches
            )
        }
    } else {
        "no matches".into()
    };

    ViewState {
        visible: data.visible,
        query: data.query.clone(),
        selected_index: data.selected_index,
        total_matches: data.total_matches,
        results: data
            .results
            .iter()
            .map(|result| ViewItem {
                id: result.project.id.clone(),
                name: result.project.name.clone(),
                path: result.project.path.clone(),
                aliases: result.project.aliases.clone(),
                language: result.project.language.clone(),
                is_debug: result.project.id.starts_with("debug-"),
            })
            .collect(),
        footer,
        cycle_id: data.active_cycle_id.clone(),
    }
}

fn config_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".project-manager.json"))
        .ok_or_else(|| "no home directory".into())
}

fn load_config() -> AppConfig {
    if let Ok(path) = config_path() {
        if let Ok(data) = fs::read(&path) {
            if let Ok(config) = serde_json::from_slice::<AppConfig>(&data) {
                return config;
            }
        }
    }
    AppConfig {
        projects: load_fixture_projects(),
        shortcut: ShortcutConfig::default(),
    }
}

fn save_config_file(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    let tmp_path = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
    let mut file = File::create(&tmp_path).map_err(|error| format!("create tmp: {error}"))?;
    file.write_all(&data)
        .map_err(|error| format!("write tmp: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync tmp: {error}"))?;
    fs::rename(&tmp_path, &path).map_err(|error| format!("rename config: {error}"))?;
    Ok(())
}

fn load_fixture_projects() -> Vec<Project> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                candidates.push(contents_dir.join("Resources/projects.json"));
            }
        }
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("shared/projects.json"));
        candidates.push(current.join("../shared/projects.json"));
    }

    for path in candidates {
        if let Ok(data) = fs::read(path) {
            if let Ok(projects) = serde_json::from_slice::<Vec<Project>>(&data) {
                return projects;
            }
        }
    }
    Vec::new()
}

#[tauri::command]
fn frontend_ready(state: tauri::State<'_, AppState>) -> ViewState {
    let data = state.inner.lock().unwrap();
    view_state(&data)
}

#[tauri::command]
fn frontend_loaded(app: AppHandle, state: tauri::State<'_, AppState>) {
    state.logger.log("frontend_loaded", None, json!({}));
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = window.center();
    }
}

#[tauri::command]
fn palette_rendered(
    state: tauri::State<'_, AppState>,
    cycle_id: Option<String>,
    frontend_apply_to_render_ms: Option<f64>,
) {
    state.logger.log(
        "palette_rendered",
        cycle_id.as_deref(),
        json!({ "frontend_apply_to_render_ms": frontend_apply_to_render_ms }),
    );
}

#[tauri::command]
fn handle_key(app: AppHandle, state: tauri::State<'_, AppState>, input: KeyInput) -> ViewState {
    let mut hide = false;
    let mut open_settings = false;
    let mut open_request = None;
    let mut search_log = None;
    let mut input_log = None;
    let mut selection_log = None;
    let view = {
        let mut data = state.inner.lock().unwrap();
        match input.key.as_str() {
            "escape" | "toggle" => {
                data.visible = false;
                hide = true;
            }
            "settings" => {
                data.visible = false;
                hide = true;
                open_settings = true;
            }
            "backspace" | "delete" => {
                if !data.query.is_empty() {
                    let input_start = Instant::now();
                    data.query.pop();
                    let outcome = run_search(&mut data);
                    search_log = Some(outcome);
                    input_log = Some(input_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "next" => {
                if !data.results.is_empty() {
                    let start = Instant::now();
                    data.selected_index = (data.selected_index + 1)
                        .min(data.results.len() as i32 - 1)
                        .max(0);
                    data.active_scenario = "navigation".into();
                    selection_log = Some(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "previous" => {
                if !data.results.is_empty() {
                    let start = Instant::now();
                    data.selected_index = (data.selected_index - 1).max(0);
                    data.active_scenario = "navigation".into();
                    selection_log = Some(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "enter" => {
                if !data.results.is_empty() && data.selected_index >= 0 {
                    let selected_index = (data.selected_index as usize).min(data.results.len() - 1);
                    let project = data.results[selected_index].project.clone();
                    open_request = Some((
                        project,
                        selected_index,
                        data.query.clone(),
                        data.active_scenario.clone(),
                        data.active_cycle_id.clone(),
                    ));
                    data.visible = false;
                    hide = true;
                }
            }
            key if key.starts_with("char:") => {
                let value = key.trim_start_matches("char:");
                if !value.is_empty() {
                    let input_start = Instant::now();
                    data.query.push_str(value);
                    let outcome = run_search(&mut data);
                    search_log = Some(outcome);
                    input_log = Some(input_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            _ => {}
        }
        view_state(&data)
    };

    if let Some(outcome) = search_log {
        log_search(&state, &view, &outcome);
    }
    if let Some(duration_ms) = input_log {
        state.logger.log(
            "input_processed",
            view.cycle_id.as_deref(),
            json!({
                "metric": "input_to_result_ms",
                "duration_ms": duration_ms,
                "query": view.query,
                "result_count": view.results.len(),
            }),
        );
    }
    if let Some(duration_ms) = selection_log {
        state.logger.log(
            "selection_moved",
            view.cycle_id.as_deref(),
            json!({
                "metric": "selection_move_ms",
                "duration_ms": duration_ms,
                "query": view.query,
                "selected_index": view.selected_index,
                "scenario": "navigation",
            }),
        );
    }
    if hide {
        hide_palette_window(&app);
    }
    if open_settings {
        open_settings_window_impl(&app);
    }
    if let Some((project, selected_index, query, scenario, cycle_id)) = open_request {
        open_project_impl(
            &app,
            &state,
            project,
            selected_index,
            query,
            scenario,
            cycle_id,
        );
    }
    view
}

#[tauri::command]
fn select_index(state: tauri::State<'_, AppState>, index: usize) -> ViewState {
    let mut data = state.inner.lock().unwrap();
    if index < data.results.len() {
        data.selected_index = index as i32;
    }
    view_state(&data)
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> AppConfig {
    let data = state.inner.lock().unwrap();
    AppConfig {
        projects: data.projects.clone(),
        shortcut: ShortcutConfig::default(),
    }
}

#[tauri::command]
fn save_config(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<ViewState, String> {
    save_config_file(&config)?;
    let view = {
        let mut data = state.inner.lock().unwrap();
        data.reload_projects(config.projects);
        view_state(&data)
    };
    apply_state_to_main(&app, &view);
    Ok(view)
}

#[tauri::command]
fn browse_folder() -> Option<String> {
    run_osascript(r#"POSIX path of (choose folder with prompt "Select a project folder")"#)
}

#[tauri::command]
fn browse_workspace_file() -> Option<String> {
    run_osascript(r#"POSIX path of (choose file with prompt "Select a workspace file")"#).filter(
        |path| {
            Path::new(path)
                .extension()
                .is_some_and(|ext| ext == "code-workspace")
        },
    )
}

#[tauri::command]
fn open_settings_window(app: AppHandle) {
    open_settings_window_impl(&app);
}

#[tauri::command]
fn close_settings_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.close();
    }
}

fn run_osascript(script: &str) -> Option<String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || !Path::new(&path).exists() {
        None
    } else {
        Some(path)
    }
}

fn log_search(state: &AppState, view: &ViewState, outcome: &SearchOutcome) {
    state.logger.log(
        "search_completed",
        view.cycle_id.as_deref(),
        json!({
            "metric": "search_ms",
            "duration_ms": outcome.duration_ms,
            "query": view.query,
            "result_count": view.results.len(),
            "alias_hit": outcome.alias_hit,
            "scenario": outcome.scenario,
        }),
    );
}

fn open_project_impl(
    app: &AppHandle,
    state: &AppState,
    project: Project,
    selected_index: usize,
    query: String,
    scenario: String,
    cycle_id: Option<String>,
) {
    state
        .logger
        .log("open_requested", cycle_id.as_deref(), json!({}));
    let start = Instant::now();
    if project.id == "debug-switch-to-appkit" {
        state.logger.log(
            "debug_switch_requested",
            cycle_id.as_deref(),
            json!({ "target": "AppKitBench", "query": query }),
        );
        let result = Command::new("/usr/bin/open")
            .args(["-a", "/Applications/AppKitBench.app"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if result.is_ok() {
            state.logger.log(
                "debug_switch_dispatched",
                cycle_id.as_deref(),
                json!({
                    "target": "AppKitBench",
                    "duration_ms": start.elapsed().as_secs_f64() * 1000.0,
                }),
            );
        } else if let Err(error) = result {
            state.logger.log(
                "debug_switch_failed",
                cycle_id.as_deref(),
                json!({ "target": "AppKitBench", "error": error.to_string() }),
            );
        }
        app.exit(0);
        return;
    }

    let paths = if project.open_paths.is_empty() {
        vec![project.path.clone()]
    } else {
        project.open_paths.clone()
    };
    let result = zed_command()
        .args(paths)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match result {
        Ok(_) => state.logger.log(
            "open_dispatched",
            cycle_id.as_deref(),
            json!({
                "metric": "open_dispatch_ms",
                "duration_ms": start.elapsed().as_secs_f64() * 1000.0,
                "project_id": project.id,
                "query": query,
                "scenario": scenario,
                "selected_index": selected_index,
            }),
        ),
        Err(error) => state.logger.log(
            "open_dispatch_failed",
            cycle_id.as_deref(),
            json!({ "project_id": project.id, "error": error.to_string() }),
        ),
    }
}

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

fn open_settings_window_impl(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    use tauri::WebviewWindowBuilder;
    let _ = WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("TauriBench Settings")
    .inner_size(720.0, 520.0)
    .resizable(true)
    .center()
    .build();
}

fn show_palette<R: Runtime>(app: &AppHandle<R>, source: &str) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let (view, should_hide, search_log) = {
        let state = app.state::<AppState>();
        let mut data = state.inner.lock().unwrap();
        if data.visible {
            data.visible = false;
            (view_state(&data), true, None)
        } else {
            let cycle_id = format!("{source}-{}", now_id());
            data.visible = true;
            data.active_cycle_id = Some(cycle_id.clone());
            data.query.clear();
            let outcome = run_search(&mut data);
            state.logger.log(
                "hotkey_received",
                Some(&cycle_id),
                json!({ "source": source }),
            );
            (view_state(&data), false, Some(outcome))
        }
    };

    if should_hide {
        let _ = window.hide();
        return;
    }

    let state = app.state::<AppState>();
    if let Some(outcome) = search_log {
        log_search(&state, &view, &outcome);
    }
    let emit_start = Instant::now();
    apply_state_to_window(&window, &view);
    state.logger.log(
        "state_emit_completed",
        view.cycle_id.as_deref(),
        json!({
            "metric": "state_emit_ms",
            "duration_ms": emit_start.elapsed().as_secs_f64() * 1000.0,
        }),
    );
    state
        .logger
        .log("native_show_requested", view.cycle_id.as_deref(), json!({}));
    let native_show_start = Instant::now();
    let _ = window.show();
    let _ = window.set_focus();
    state.logger.log(
        "native_show_completed",
        view.cycle_id.as_deref(),
        json!({
            "metric": "native_show_ms",
            "duration_ms": native_show_start.elapsed().as_secs_f64() * 1000.0,
        }),
    );
}

fn hide_palette<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    {
        let state = app.state::<AppState>();
        let mut data = state.inner.lock().unwrap();
        data.visible = false;
    }
}

fn hide_palette_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn apply_state_to_main(app: &AppHandle, view: &ViewState) {
    if let Some(window) = app.get_webview_window("main") {
        apply_state_to_window(&window, view);
    }
}

fn apply_state_to_window<R: Runtime>(window: &tauri::WebviewWindow<R>, view: &ViewState) {
    if let Ok(payload) = serde_json::to_string(view) {
        let _ = window.eval(format!("window.__TAURI_BENCH_APPLY_STATE?.({payload});"));
    }
}

fn now_id() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub fn run() {
    let config = load_config();
    let project_count = config.projects.len();
    let logger = BenchLogger::new();
    logger.log(
        "app_ready",
        None,
        json!({ "project_count": project_count, "source": "backend" }),
    );

    tauri::Builder::default()
        .manage(AppState {
            inner: Mutex::new(LauncherData::new(config.projects)),
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
            frontend_ready,
            frontend_loaded,
            palette_rendered,
            handle_key,
            select_index,
            get_config,
            save_config,
            browse_folder,
            browse_workspace_file,
            open_settings_window,
            close_settings_window
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Focused(false)) {
                hide_palette(window.app_handle());
            }
        })
        .setup(|app| {
            app.handle()
                .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                    show_palette(app, "single-instance");
                }))?;
            app.global_shortcut().register("Control+KeyM")?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_position(tauri::Position::Physical(
                    tauri::PhysicalPosition::new(-10_000, -10_000),
                ));
                let _ = window.show();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
