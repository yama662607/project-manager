mod config;
mod ui;

use anyhow::{Context, Result};
use crossterm::{
    event::{
        DisableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use config::{load_config, is_project_registered};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;
use ui::{app::App, screens::Screen};

fn main() -> Result<()> {
    // ターミナル設定
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, DisableMouseCapture)?;

    // 拡張キーボードプロトコルを有効化（Ctrl+MとEnterを区別するため）。
    // 対応端末でのみ有効化し、非対応端末では従来動作にフォールバックする。
    let keyboard_enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if keyboard_enhanced {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 設定を読み込み
    let config = load_config();

    // 現在のディレクトリを取得
    let current_dir = std::env::current_dir()
        .context("現在のディレクトリの取得に失敗しました")?;

    let current_dir_str = current_dir
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("ディレクトリパスの変換に失敗しました"))?
        .to_string();

    // Quick Addチェック: 未登録の場合はQuick Add画面を起動
    let initial_screen = if !is_project_registered(&config, &current_dir_str) {
        let dir_name = current_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        Screen::QuickAdd {
            name: dir_name,
            path: current_dir_str,
            aliases: String::new(),
        }
    } else {
        Screen::ProjectList { selected_index: 0 }
    };

    // アプリケーション作成
    let mut app = App::new(config, initial_screen);

    // メインループ
    let res = app.run(&mut terminal);

    // ターミナル復元
    if keyboard_enhanced {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("エラー: {}", e);
    }

    Ok(())
}
