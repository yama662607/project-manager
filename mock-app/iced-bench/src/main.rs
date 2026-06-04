use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use iced::{
    keyboard::{self, key, Event as KeyboardEvent, Key},
    widget::{button, column, container, row, scrollable, text},
    window, Background, Color, Element, Length, Point, Size, Subscription, Task, Theme,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

const APP_NAME: &str = "iced";
const LOCK_FILE_NAME: &str = "project-launcher-iced-bench.lock";
static SINGLE_INSTANCE: OnceLock<Mutex<Option<SingleInstance>>> = OnceLock::new();

#[derive(Clone, Deserialize)]
struct Project {
    id: String,
    name: String,
    path: String,
    tags: Vec<String>,
    language: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(rename = "lastOpenedAt")]
    _last_opened_at: String,
}

#[derive(Clone)]
struct IndexedProject {
    project: Project,
    id: String,
    name: String,
    path: String,
    tags: String,
    aliases: String,
}

#[derive(Clone)]
struct SearchResult {
    project: Project,
    score: i32,
}

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
            .open(dir.join(format!("iced-{}.jsonl", std::process::id())))
            .expect("open metrics log");
        Self {
            start: Instant::now(),
            file: Mutex::new(file),
        }
    }

    fn log(&self, event: &str, cycle_id: Option<&str>, fields: Value) {
        let mut payload = match fields {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        payload.insert("app".into(), json!(APP_NAME));
        payload.insert("event".into(), json!(event));
        payload.insert("mono_ns".into(), json!(self.start.elapsed().as_nanos()));
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

struct State {
    _single_instance: Option<SingleInstance>,
    hotkey_manager: Option<GlobalHotKeyManager>,
    window_id: Option<window::Id>,
    indexed: Vec<IndexedProject>,
    alias_index: HashMap<String, usize>,
    logger: BenchLogger,
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
    total_matches: usize,
    footer: String,
    active_cycle_id: Option<String>,
    active_scenario: String,
    benchmark_remaining: usize,
    escape_hotkey_registered: bool,
    render_pending: bool,
}

struct SingleInstance {
    path: PathBuf,
}

impl SingleInstance {
    fn acquire() -> Option<Self> {
        let path = std::env::temp_dir().join(LOCK_FILE_NAME);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                Some(Self { path })
            }
            Err(_) => {
                if stale_lock(&path) {
                    let _ = fs::remove_file(&path);
                    return Self::acquire();
                }
                None
            }
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone)]
enum Message {
    Key(KeyboardEvent),
    Tick,
    ShowPalette(String),
    PaletteRendered,
    RunBenchmark,
    OpenSelected,
    Focused(Option<window::Id>),
}

fn main() -> iced::Result {
    let Some(single_instance) = SingleInstance::acquire() else {
        return Ok(());
    };
    let _ = SINGLE_INSTANCE.set(Mutex::new(Some(single_instance)));
    iced::application(State::new, update, view)
        .title(title)
        .theme(theme)
        .subscription(subscription)
        .window(window::Settings {
            size: Size::new(760.0, 520.0),
            resizable: false,
            decorations: false,
            level: window::Level::AlwaysOnTop,
            ..window::Settings::default()
        })
        .run()
}

impl State {
    fn new() -> Self {
        let single_instance = SINGLE_INSTANCE
            .get()
            .and_then(|lock| lock.lock().ok().and_then(|mut guard| guard.take()));
        let logger = BenchLogger::new();
        let projects = load_projects();
        let indexed = projects
            .into_iter()
            .map(|project| IndexedProject {
                id: project.id.to_lowercase(),
                name: project.name.to_lowercase(),
                path: project.path.to_lowercase(),
                tags: project.tags.join(" ").to_lowercase(),
                aliases: project.aliases.join(" ").to_lowercase(),
                project,
            })
            .collect::<Vec<_>>();
        let mut alias_index = HashMap::new();
        for (index, item) in indexed.iter().enumerate() {
            for alias in &item.project.aliases {
                alias_index.entry(alias.to_lowercase()).or_insert(index);
            }
        }

        let manager = GlobalHotKeyManager::new().ok();
        if let Some(manager) = &manager {
            let _ = manager.register(show_hotkey());
        }

        logger.log("app_ready", None, json!({ "project_count": indexed.len() }));

        let mut state = Self {
            _single_instance: single_instance,
            hotkey_manager: manager,
            window_id: None,
            indexed,
            alias_index,
            logger,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            total_matches: 0,
            footer: String::new(),
            active_cycle_id: None,
            active_scenario: String::new(),
            benchmark_remaining: 0,
            escape_hotkey_registered: false,
            render_pending: false,
        };
        state.perform_search();
        state
    }

    fn show_palette(&mut self, source: &str) -> Task<Message> {
        let cycle_id = format!("{source}-{}", now_id());
        self.active_cycle_id = Some(cycle_id.clone());
        self.active_scenario.clear();
        self.logger.log(
            "hotkey_received",
            Some(&cycle_id),
            json!({ "source": source }),
        );
        self.query.clear();
        self.selected = 0;
        self.perform_search();
        if let Some(manager) = &self.hotkey_manager {
            if !self.escape_hotkey_registered && manager.register(escape_hotkey()).is_ok() {
                self.escape_hotkey_registered = true;
            }
        }
        if let Some(id) = self.window_id {
            Task::done(Message::Focused(Some(id)))
        } else {
            window::latest().map(Message::Focused)
        }
    }

    fn perform_search(&mut self) {
        let start = Instant::now();
        let normalized_query = self.query.trim().to_lowercase();
        let mut scenario = "";

        if !normalized_query.is_empty() {
            if let Some(index) = self.alias_index.get(&normalized_query) {
                if normalized_query == "a" {
                    scenario = "alias";
                }
                self.active_scenario = scenario.to_owned();
                self.total_matches = 1;
                self.results = vec![SearchResult {
                    project: self.indexed[*index].project.clone(),
                    score: 2000,
                }];
                self.selected = 0;

                let duration = start.elapsed().as_secs_f64() * 1000.0;
                self.footer = format!(
                    "alias hit - showing 1 of 1 matches - search {:.3} ms",
                    duration
                );
                self.logger.log(
                    "search_completed",
                    self.active_cycle_id.as_deref(),
                    json!({
                        "metric": "search_ms",
                        "duration_ms": duration,
                        "query": self.query,
                        "result_count": self.results.len(),
                        "alias_hit": normalized_query,
                        "scenario": scenario
                    }),
                );
                return;
            }
        }
        if normalized_query == "pr" {
            scenario = "narrowing";
        }
        self.active_scenario = scenario.to_owned();

        let tokens = self
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        if tokens.is_empty() {
            self.total_matches = self.indexed.len();
            self.results = self
                .indexed
                .iter()
                .take(50)
                .map(|item| SearchResult {
                    project: item.project.clone(),
                    score: 0,
                })
                .collect();
        } else {
            let mut matches = Vec::new();
            for item in &self.indexed {
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
                    });
                }
            }
            matches.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| a.project.name.cmp(&b.project.name))
            });
            self.total_matches = matches.len();
            matches.truncate(50);
            self.results = matches;
        }

        if self.selected >= self.results.len() {
            self.selected = self.results.len().saturating_sub(1);
        }

        let duration = start.elapsed().as_secs_f64() * 1000.0;
        self.footer = format!(
            "showing {} of {} matches - search {:.3} ms",
            self.results.len(),
            self.total_matches,
            duration
        );
        self.logger.log(
            "search_completed",
            self.active_cycle_id.as_deref(),
            json!({
                "metric": "search_ms",
                "duration_ms": duration,
                "query": self.query,
                "result_count": self.results.len(),
                "scenario": scenario
            }),
        );
    }

    fn log_input_processed(&self, duration: f64) {
        self.logger.log(
            "input_processed",
            self.active_cycle_id.as_deref(),
            json!({
                "metric": "input_to_result_ms",
                "duration_ms": duration,
                "query": self.query,
                "result_count": self.results.len(),
                "scenario": self.active_scenario
            }),
        );
    }

    fn update_query_from_key(
        &mut self,
        key: &Key,
        physical_key: &key::Physical,
        modifiers: keyboard::Modifiers,
    ) -> bool {
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return false;
        }

        let start = Instant::now();
        match key {
            Key::Named(key::Named::Backspace) => {
                if self.query.pop().is_none() {
                    return true;
                }
            }
            Key::Named(key::Named::Delete) => {
                return true;
            }
            _ => {
                let Some(character) = ascii_from_physical_key(physical_key) else {
                    return false;
                };
                self.query.push(character);
            }
        }

        self.perform_search();
        self.log_input_processed(start.elapsed().as_secs_f64() * 1000.0);
        true
    }

    fn move_selection(&mut self, offset: isize) {
        if self.results.is_empty() {
            return;
        }
        let start = Instant::now();
        self.selected = if offset > 0 {
            (self.selected + offset as usize).min(self.results.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(offset.unsigned_abs())
        };
        self.active_scenario = "navigation".to_owned();
        self.logger.log(
            "selection_moved",
            self.active_cycle_id.as_deref(),
            json!({
                "metric": "selection_move_ms",
                "duration_ms": start.elapsed().as_secs_f64() * 1000.0,
                "query": self.query,
                "selected_index": self.selected,
                "scenario": self.active_scenario
            }),
        );
    }

    fn open_selected(&mut self) {
        let Some(result) = self.results.get(self.selected) else {
            return;
        };
        let project = result.project.clone();
        let cycle_id = self
            .active_cycle_id
            .clone()
            .unwrap_or_else(|| format!("open-{}", now_id()));
        self.footer = format!("opening {}", project.name);
        self.logger
            .log("open_requested", Some(&cycle_id), json!({}));
        let _ = zed_command()
            .arg(&project.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        self.logger.log(
            "open_dispatched",
            Some(&cycle_id),
            json!({
                "project_id": project.id,
                "scenario": self.active_scenario,
                "query": self.query,
                "selected_index": self.selected
            }),
        );
    }

    fn close_palette(&mut self) -> Task<Message> {
        if let Some(manager) = &self.hotkey_manager {
            if self.escape_hotkey_registered {
                let _ = manager.unregister(escape_hotkey());
                self.escape_hotkey_registered = false;
            }
        }
        if let Some(id) = self.window_id {
            window::move_to(id, Point::new(100_000.0, 100_000.0))
        } else {
            window::latest()
                .and_then(|id| window::move_to(id, Point::new(100_000.0, 100_000.0)))
        }
    }
}

fn stale_lock(path: &Path) -> bool {
    let mut content = String::new();
    if File::open(path)
        .and_then(|mut file| file.read_to_string(&mut content))
        .is_err()
    {
        return true;
    }
    let Ok(pid) = content.trim().parse::<u32>() else {
        return true;
    };
    Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| !status.success())
        .unwrap_or(true)
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

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Key(KeyboardEvent::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) => match key.as_ref() {
            Key::Named(key::Named::Enter) => {
                state.open_selected();
                state.close_palette()
            }
            Key::Named(key::Named::Escape) => state.close_palette(),
            _ if modifiers.control()
                && !modifiers.alt()
                && !modifiers.logo()
                && matches!(physical_key, key::Physical::Code(key::Code::KeyN)) =>
            {
                state.move_selection(1);
                Task::none()
            }
            _ if modifiers.control()
                && !modifiers.alt()
                && !modifiers.logo()
                && matches!(physical_key, key::Physical::Code(key::Code::KeyP)) =>
            {
                state.move_selection(-1);
                Task::none()
            }
            Key::Named(key::Named::ArrowDown) => {
                state.move_selection(1);
                Task::none()
            }
            Key::Named(key::Named::ArrowUp) => {
                state.move_selection(-1);
                Task::none()
            }
            _ => {
                state.update_query_from_key(&key, &physical_key, modifiers);
                Task::none()
            }
        },
        Message::Key(_) => Task::none(),
        Message::Tick => {
            let mut task = Task::none();
            if state.render_pending {
                state.render_pending = false;
                if let Some(cycle_id) = &state.active_cycle_id {
                    state
                        .logger
                        .log("palette_rendered", Some(cycle_id), json!({}));
                }
            }
            for event in GlobalHotKeyEvent::receiver().try_iter() {
                if event.state() == HotKeyState::Pressed {
                    task = if event.id() == escape_hotkey().id() {
                        state.close_palette()
                    } else if event.id() == show_hotkey().id() {
                        state.show_palette("hotkey")
                    } else {
                        Task::none()
                    };
                }
            }
            if state.benchmark_remaining > 0 {
                state.benchmark_remaining -= 1;
                let queries = ["a", "pr", "api", "web", "manager", "ios", "zed"];
                let index = 100 - state.benchmark_remaining;
                state.query = queries[index % queries.len()].to_owned();
                task = state.show_palette("benchmark");
                if state.benchmark_remaining == 0 {
                    state
                        .logger
                        .log("benchmark_cycle_completed", None, json!({ "count": 100 }));
                }
            }
            task
        }
        Message::ShowPalette(source) => state.show_palette(&source),
        Message::PaletteRendered => Task::none(),
        Message::RunBenchmark => {
            state.benchmark_remaining = 100;
            Task::none()
        }
        Message::OpenSelected => {
            state.open_selected();
            state.close_palette()
        }
        Message::Focused(id) => {
            let focus = match id {
                Some(id) => {
                    state.window_id = Some(id);
                    state.render_pending = true;
                    Task::batch(vec![
                        window::move_to(id, Point::new(320.0, 120.0)),
                        window::gain_focus(id),
                    ])
                }
                None => Task::done(Message::PaletteRendered),
            };
            focus
        }
    }
}

fn title(_state: &State) -> String {
    "IcedBench".to_owned()
}

fn theme(_state: &State) -> Theme {
    Theme::Dark
}

fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::batch(vec![
        iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick),
        keyboard::listen().map(Message::Key),
    ])
}

fn show_hotkey() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL), Code::KeyM)
}

fn escape_hotkey() -> HotKey {
    HotKey::new(None, Code::Escape)
}

fn ascii_from_physical_key(physical_key: &key::Physical) -> Option<char> {
    match physical_key {
        key::Physical::Code(key::Code::KeyA) => Some('a'),
        key::Physical::Code(key::Code::KeyB) => Some('b'),
        key::Physical::Code(key::Code::KeyC) => Some('c'),
        key::Physical::Code(key::Code::KeyD) => Some('d'),
        key::Physical::Code(key::Code::KeyE) => Some('e'),
        key::Physical::Code(key::Code::KeyF) => Some('f'),
        key::Physical::Code(key::Code::KeyG) => Some('g'),
        key::Physical::Code(key::Code::KeyH) => Some('h'),
        key::Physical::Code(key::Code::KeyI) => Some('i'),
        key::Physical::Code(key::Code::KeyJ) => Some('j'),
        key::Physical::Code(key::Code::KeyK) => Some('k'),
        key::Physical::Code(key::Code::KeyL) => Some('l'),
        key::Physical::Code(key::Code::KeyM) => Some('m'),
        key::Physical::Code(key::Code::KeyN) => Some('n'),
        key::Physical::Code(key::Code::KeyO) => Some('o'),
        key::Physical::Code(key::Code::KeyP) => Some('p'),
        key::Physical::Code(key::Code::KeyQ) => Some('q'),
        key::Physical::Code(key::Code::KeyR) => Some('r'),
        key::Physical::Code(key::Code::KeyS) => Some('s'),
        key::Physical::Code(key::Code::KeyT) => Some('t'),
        key::Physical::Code(key::Code::KeyU) => Some('u'),
        key::Physical::Code(key::Code::KeyV) => Some('v'),
        key::Physical::Code(key::Code::KeyW) => Some('w'),
        key::Physical::Code(key::Code::KeyX) => Some('x'),
        key::Physical::Code(key::Code::KeyY) => Some('y'),
        key::Physical::Code(key::Code::KeyZ) => Some('z'),
        key::Physical::Code(key::Code::Digit0) => Some('0'),
        key::Physical::Code(key::Code::Digit1) => Some('1'),
        key::Physical::Code(key::Code::Digit2) => Some('2'),
        key::Physical::Code(key::Code::Digit3) => Some('3'),
        key::Physical::Code(key::Code::Digit4) => Some('4'),
        key::Physical::Code(key::Code::Digit5) => Some('5'),
        key::Physical::Code(key::Code::Digit6) => Some('6'),
        key::Physical::Code(key::Code::Digit7) => Some('7'),
        key::Physical::Code(key::Code::Digit8) => Some('8'),
        key::Physical::Code(key::Code::Digit9) => Some('9'),
        key::Physical::Code(key::Code::Minus) => Some('-'),
        key::Physical::Code(key::Code::Space) => Some(' '),
        _ => None,
    }
}

fn view(state: &State) -> Element<'_, Message> {
    let search_label = if state.query.is_empty() {
        "Search projects"
    } else {
        state.query.as_str()
    };
    let search_color = if state.query.is_empty() {
        Color::from_rgb8(120, 120, 128)
    } else {
        Color::from_rgb8(230, 230, 240)
    };
    let search = container(text(search_label).size(22).color(search_color))
        .padding(10)
        .height(42)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(28, 31, 38))),
            ..container::Style::default()
        });

    let mut rows = column![];
    for (index, result) in state.results.iter().enumerate() {
        let label = if index == state.selected {
            format!(
                "▸ {}  -  {}  -  {}  -  {}",
                result.project.name,
                result.project.id,
                result.project.language,
                result.project.path
            )
        } else {
            format!(
                "  {}  -  {}  -  {}  -  {}",
                result.project.name,
                result.project.id,
                result.project.language,
                result.project.path
            )
        };
        rows = rows.push(
            if index == state.selected {
                button(text(label).size(14))
                    .width(Length::Fill)
                    .on_press(Message::OpenSelected)
                    .style(|_, _| {
                        button::Style {
                            background: Some(Background::Color(Color::from_rgba8(212, 168, 83, 0.28))),
                            text_color: Color::from_rgb8(255, 255, 255),
                            ..button::Style::default()
                        }
                    })
            } else {
                button(text(label).size(14))
                    .width(Length::Fill)
                    .on_press(Message::OpenSelected)
            },
        );
    }

    let controls = row![
        button("Show").on_press(Message::ShowPalette("menu".into())),
        button("Run Benchmark").on_press(Message::RunBenchmark),
    ]
    .spacing(10);

    let content = column![
        search,
        scrollable(rows).height(Length::Fill),
        text(&state.footer).size(11),
        controls
    ]
    .spacing(14);

    container(content)
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            text_color: Some(Color::from_rgb8(230, 230, 240)),
            background: Some(Background::Color(Color::from_rgb8(20, 22, 27))),
            ..container::Style::default()
        })
        .into()
}

fn score_token(token: &str, item: &IndexedProject) -> i32 {
    if item.id == token {
        return 1400;
    }
    if token.len() >= 3 && item.id.contains(token) {
        return 1000;
    }
    if item.name.starts_with(token) {
        return 1200 - item.name.len().min(300) as i32;
    }
    if token.len() == 1 && word_has_prefix(token, &item.name) {
        return 600;
    }
    if token.len() >= 3 && item.name.contains(token) {
        return 900 - item.name.len().min(250) as i32;
    }
    if token.len() >= 3 && item.tags.contains(token) {
        return 700;
    }
    if token.len() >= 3 && item.aliases.contains(token) {
        return 650;
    }
    if token.len() >= 3 && item.path.contains(token) {
        return 450;
    }
    if token.chars().any(|char| char.is_ascii_digit()) {
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

fn word_has_prefix(token: &str, candidate: &str) -> bool {
    candidate
        .split(|char: char| !char.is_ascii_alphanumeric())
        .any(|word| word.starts_with(token))
}

fn fuzzy_contains(token: &str, candidate: &str) -> bool {
    let mut chars = candidate.chars();
    for needle in token.chars() {
        if !chars.any(|char| char == needle) {
            return false;
        }
    }
    true
}

fn load_projects() -> Vec<Project> {
    for path in fixture_candidates() {
        if let Ok(data) = fs::read(path) {
            if let Ok(projects) = serde_json::from_slice::<Vec<Project>>(&data) {
                return projects;
            }
        }
    }
    Vec::new()
}

fn fixture_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos) = exe.parent() {
            if let Some(contents) = macos.parent() {
                paths.push(contents.join("Resources/projects.json"));
            }
        }
    }
    if let Ok(current) = std::env::current_dir() {
        paths.push(current.join("shared/projects.json"));
        paths.push(current.join("../shared/projects.json"));
    }
    paths
}

fn now_id() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".into())
}
