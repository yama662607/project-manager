use super::screens::{FormField, Screen};
use crate::config::{add_project, delete_project, update_project, AppConfig, Project};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{backend::Backend, Terminal, text::{Line, Text}, widgets::Wrap};

/// アプリケーションの状態
pub struct App {
    /// 設定
    pub config: AppConfig,
    /// 現在の画面
    pub screen: Screen,
    /// 終了フラグ
    pub should_quit: bool,
    /// エラーメッセージ
    pub error_message: Option<String>,
}

impl App {
    /// 新しいアプリケーションを作成
    pub fn new(config: AppConfig, screen: Screen) -> Self {
        Self {
            config,
            screen,
            should_quit: false,
            error_message: None,
        }
    }

    /// メインループを実行
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|f| self.draw(f))?;
            self.handle_events()?;
        }
        Ok(())
    }

    /// 画面を描画
    fn draw(&self, frame: &mut ratatui::Frame) {
        match &self.screen {
            Screen::ProjectList { selected_index } => {
                self.draw_project_list(frame, *selected_index);
            }
            Screen::QuickAdd { name, path, aliases } => {
                self.draw_quick_add(frame, name, path, aliases);
            }
            Screen::AddForm {
                name,
                path,
                aliases,
                tags,
                language,
                current_field,
            } => {
                self.draw_add_form(frame, name, path, aliases, tags, language, *current_field);
            }
            Screen::EditForm {
                index,
                name,
                path,
                aliases,
                tags,
                language,
                current_field,
            } => {
                self.draw_edit_form(frame, *index, name, path, aliases, tags, language, *current_field);
            }
            Screen::DeleteConfirm { project_name, .. } => {
                self.draw_delete_confirm(frame, project_name);
            }
        }

        // エラーメッセージ表示
        if let Some(error) = &self.error_message {
            self.draw_error(frame, error);
        }
    }

    /// プロジェクト一覧画面を描画
    fn draw_project_list(&self, frame: &mut ratatui::Frame, selected_index: usize) {
        use ratatui::{
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
        };

        let size = frame.area();

        // ヘッダー
        let total = self.config.projects.len();
        let header = vec![
            Line::from(vec![
                Span::styled("Project Manager", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  ({}/{})", (selected_index + 1).min(total.max(1)), total),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::styled("n: 新規 | e: 編集 | d: 削除 | q: 終了", Style::default().fg(Color::DarkGray)),
            ]),
        ];

        // プロジェクト一覧（選択行のハイライトはListのhighlight_styleに任せる）
        let projects: Vec<ListItem> = self.config.projects.iter().map(|p| {
            let mut lines = vec![
                Line::from(Span::styled(format!("📁 {}", p.name), Style::default().fg(Color::Green))),
                Line::from(Span::styled(format!("   {}", p.path), Style::default().fg(Color::DarkGray))),
            ];

            if !p.aliases.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   Aliases: {}", p.aliases.join(", ")),
                    Style::default().fg(Color::Cyan),
                )));
            }

            ListItem::new(lines)
        }).collect();

        let list = List::new(projects)
            .block(Block::default().borders(Borders::ALL).title("Projects"))
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(size);

        // ヘッダー描画
        let header_paragraph = Paragraph::new(Text::from(header))
            .alignment(Alignment::Center);
        frame.render_widget(header_paragraph, chunks[0]);

        // リスト描画（選択項目が画面内に入るようスクロール）
        let mut state = ListState::default();
        if total > 0 {
            state.select(Some(selected_index.min(total - 1)));
        }
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    /// Quick Add画面を描画
    fn draw_quick_add(&self, frame: &mut ratatui::Frame, name: &str, path: &str, aliases: &str) {
        use ratatui::{
            layout::{Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph},
        };

        let size = frame.area();

        let content = vec![
            Line::from(vec![
                Span::styled("Quick Add - 新しいプロジェクトを登録", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("名前: ", Style::default().fg(Color::Green)),
                Span::raw(name),
            ]),
            Line::from(vec![
                Span::styled("パス: ", Style::default().fg(Color::Green)),
                Span::raw(path),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("エイリアス (任意): ", Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled(" aliases> ", Style::default().fg(Color::Cyan)),
                Span::raw(aliases),
                Span::styled("█", Style::default().fg(Color::White)), // カーソル
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Enter: 保存 | Escape: キャンセル", Style::default().fg(Color::DarkGray)),
            ]),
        ];

        let content_len = content.len();
        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Quick Add"))
            .wrap(Wrap { trim: false });

        let centered = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(content_len as u16 + 4),
                Constraint::Min(0),
            ].as_ref())
            .split(size);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ].as_ref())
            .split(centered[0]);

        frame.render_widget(paragraph, horizontal[1]);
    }

    /// 追加フォーム画面を描画
    fn draw_add_form(
        &self,
        frame: &mut ratatui::Frame,
        name: &str,
        path: &str,
        aliases: &str,
        tags: &str,
        language: &str,
        current_field: usize,
    ) {
        use ratatui::{
            layout::{Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph, Wrap},
        };

        let size = frame.area();

        let fields = FormField::all();
        let values = [name, path, aliases, tags, language];

        let mut content = vec![
            Line::from(vec![
                Span::styled("新しいプロジェクトを追加", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ];

        for (i, field) in fields.iter().enumerate() {
            let is_current = i == current_field;
            let field_style = if is_current {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            content.push(Line::from(vec![
                Span::styled(format!("{}: ", field.label()), field_style),
                Span::raw(values[i]),
            ]));

            if is_current {
                content.push(Line::from(vec![
                    Span::styled(format!("  {}> ", field.label()), Style::default().fg(Color::Cyan)),
                    Span::styled("█", Style::default().fg(Color::White)),
                ]));
            }
        }

        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("Enter: 次/保存 | Escape: キャンセル | Ctrl+N/P: フィールド移動", Style::default().fg(Color::DarkGray)),
        ]));

        let content_len = content.len();
        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Add Project"))
            .wrap(Wrap { trim: false });

        let centered = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(content_len as u16 + 4),
                Constraint::Min(0),
            ].as_ref())
            .split(size);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ].as_ref())
            .split(centered[0]);

        frame.render_widget(paragraph, horizontal[1]);
    }

    /// 編集フォーム画面を描画
    fn draw_edit_form(
        &self,
        frame: &mut ratatui::Frame,
        index: usize,
        name: &str,
        path: &str,
        aliases: &str,
        tags: &str,
        language: &str,
        current_field: usize,
    ) {
        use ratatui::{
            layout::{Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph, Wrap},
        };

        let size = frame.area();

        let fields = FormField::all();
        let values = [name, path, aliases, tags, language];

        let mut content = vec![
            Line::from(vec![
                Span::styled(
                    format!("プロジェクトを編集 (#{} )", index + 1),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        for (i, field) in fields.iter().enumerate() {
            let is_current = i == current_field;
            let field_style = if is_current {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            content.push(Line::from(vec![
                Span::styled(format!("{}: ", field.label()), field_style),
                Span::raw(values[i]),
            ]));

            if is_current {
                content.push(Line::from(vec![
                    Span::styled(format!("  {}> ", field.label()), Style::default().fg(Color::Cyan)),
                    Span::styled("█", Style::default().fg(Color::White)),
                ]));
            }
        }

        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("Enter: 次/保存 | Escape: キャンセル | Ctrl+N/P: フィールド移動", Style::default().fg(Color::DarkGray)),
        ]));

        let content_len = content.len();
        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Edit Project"))
            .wrap(Wrap { trim: false });

        let centered = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(content_len as u16 + 4),
                Constraint::Min(0),
            ].as_ref())
            .split(size);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ].as_ref())
            .split(centered[0]);

        frame.render_widget(paragraph, horizontal[1]);
    }

    /// 削除確認ダイアログを描画
    fn draw_delete_confirm(&self, frame: &mut ratatui::Frame, project_name: &str) {
        use ratatui::{
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Style},
            text::Span,
            widgets::{Block, Borders, Paragraph, Wrap},
        };

        let size = frame.area();

        let content = vec![
            Line::from(vec![
                Span::styled(
                    format!("「{}」を削除しますか？", project_name),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(" (y/n)"),
            ]),
        ];

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(0),
            ].as_ref())
            .split(size);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ].as_ref())
            .split(chunks[0]);

        frame.render_widget(paragraph, horizontal[1]);
    }

    /// エラーメッセージを描画
    fn draw_error(&self, frame: &mut ratatui::Frame, error: &str) {
        use ratatui::{
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph, Wrap},
        };

        let size = frame.area();

        let content = vec![
            Line::from(vec![
                Span::styled("エラー: ", Style::default().fg(Color::Red).add_modifier(ratatui::style::Modifier::BOLD)),
                Span::styled(error, Style::default().fg(Color::Red)),
            ]),
        ];

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
            .split(size);

        frame.render_widget(paragraph, chunks[1]);
    }

    /// イベントを処理
    fn handle_events(&mut self) -> Result<()> {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    self.handle_key(key)?;
                }
            }
        }
        Ok(())
    }

    /// キー入力を処理
    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // エラーメッセージをクリア
        self.error_message = None;

        // screenをcloneして借用を回避
        let screen_clone = self.screen.clone();

        match screen_clone {
            Screen::ProjectList { selected_index } => {
                let mut idx = selected_index;
                self.handle_project_list_key(key, &mut idx)?;
                // ハンドラーが画面遷移していなければ、selected_indexを反映
                if matches!(self.screen, Screen::ProjectList { .. }) {
                    self.screen = Screen::ProjectList { selected_index: idx };
                }
            }
            Screen::QuickAdd { name, path, aliases } => {
                let mut al = aliases;
                self.handle_quick_add_key(key, &name, &path, &mut al)?;
                // ハンドラーが画面遷移していなければ、aliasesを反映
                if matches!(self.screen, Screen::QuickAdd { .. }) {
                    self.screen = Screen::QuickAdd { name, path, aliases: al };
                }
            }
            Screen::AddForm {
                name,
                path,
                aliases,
                tags,
                language,
                current_field,
            } => {
                let mut nm = name;
                let mut ph = path;
                let mut al = aliases;
                let mut tg = tags;
                let mut lg = language;
                let mut cf = current_field;

                self.handle_add_form_key(key, &mut nm, &mut ph, &mut al, &mut tg, &mut lg, &mut cf)?;

                // ハンドラーが画面遷移していなければ、フィールド値を反映
                if matches!(self.screen, Screen::AddForm { .. }) {
                    self.screen = Screen::AddForm {
                        name: nm,
                        path: ph,
                        aliases: al,
                        tags: tg,
                        language: lg,
                        current_field: cf,
                    };
                }
            }
            Screen::EditForm {
                index,
                name,
                path,
                aliases,
                tags,
                language,
                current_field,
            } => {
                let mut nm = name;
                let mut ph = path;
                let mut al = aliases;
                let mut tg = tags;
                let mut lg = language;
                let mut cf = current_field;

                self.handle_edit_form_key(key, index, &mut nm, &mut ph, &mut al, &mut tg, &mut lg, &mut cf)?;

                // ハンドラーが画面遷移していなければ、フィールド値を反映
                if matches!(self.screen, Screen::EditForm { .. }) {
                    self.screen = Screen::EditForm {
                        index,
                        name: nm,
                        path: ph,
                        aliases: al,
                        tags: tg,
                        language: lg,
                        current_field: cf,
                    };
                }
            }
            Screen::DeleteConfirm { index, project_name } => {
                self.handle_delete_confirm_key(key, index)?;
                // ハンドラーが画面遷移していなければ、そのまま保持
                if matches!(self.screen, Screen::DeleteConfirm { .. }) {
                    self.screen = Screen::DeleteConfirm { index, project_name };
                }
            }
        }

        Ok(())
    }

    /// プロジェクト一覧画面のキー入力を処理
    fn handle_project_list_key(&mut self, key: KeyEvent, selected_index: &mut usize) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('\x1b') => {
                self.should_quit = true;
            }
            // Ctrl+N / Ctrl+P: ナビゲーション（無条件Charより先に判定）
            KeyCode::Char('n') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *selected_index < self.config.projects.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if *selected_index < self.config.projects.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            KeyCode::Up => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            // プレーンなキー（修飾キーなし）
            KeyCode::Char('n') => {
                self.screen = Screen::add_form();
            }
            KeyCode::Char('e') => {
                if let Some(project) = self.config.projects.get(*selected_index) {
                    let index = *selected_index;
                    self.screen = Screen::EditForm {
                        index,
                        name: project.name.clone(),
                        path: project.path.clone(),
                        aliases: project.aliases.join(", "),
                        tags: project.tags.join(", "),
                        language: project.language.clone(),
                        current_field: 0,
                    };
                }
            }
            KeyCode::Char('d') => {
                if let Some(project) = self.config.projects.get(*selected_index) {
                    let index = *selected_index;
                    self.screen = Screen::DeleteConfirm {
                        index,
                        project_name: project.name.clone(),
                    };
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Quick Add画面のキー入力を処理
    fn handle_quick_add_key(&mut self, key: KeyEvent, name: &str, path: &str, aliases: &mut String) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('\r') | KeyCode::Char('\n') => {
                // プロジェクトを保存
                let project = Project::new(
                    name.to_string(),
                    path.to_string(),
                    if aliases.is_empty() { vec![] } else { aliases.split(',').map(|s| s.trim().to_string()).collect() },
                    vec![],
                    None,
                );

                if let Err(e) = add_project(&mut self.config, project) {
                    self.error_message = Some(format!("保存に失敗: {}", e));
                } else {
                    // 追加した項目を選択した状態で一覧画面へ
                    let last = self.config.projects.len().saturating_sub(1);
                    self.screen = Screen::project_list_at(last);
                }
            }
            KeyCode::Esc | KeyCode::Char('\x1b') => {
                // キャンセルして一覧画面へ
                self.screen = Screen::project_list();
            }
            KeyCode::Backspace => {
                aliases.pop();
            }
            KeyCode::Char(c) if !c.is_control() => {
                aliases.push(c);
            }
            _ => {}
        }

        Ok(())
    }

    /// 追加フォーム画面のキー入力を処理
    fn handle_add_form_key(
        &mut self,
        key: KeyEvent,
        name: &mut String,
        path: &mut String,
        aliases: &mut String,
        tags: &mut String,
        language: &mut String,
        current_field: &mut usize,
    ) -> Result<()> {
        let fields = FormField::all();
        let field_count = fields.len();

        match key.code {
            KeyCode::Enter | KeyCode::Char('\r') | KeyCode::Char('\n') => {
                if *current_field < field_count - 1 {
                    // 次のフィールドへ
                    *current_field += 1;
                    return Ok(());
                } else {
                    // 保存
                    if name.is_empty() || path.is_empty() {
                        self.error_message = Some("名前とパスは必須です".to_string());
                        return Ok(());
                    }

                    let project = Project::new(
                        name.clone(),
                        path.clone(),
                        if aliases.is_empty() { vec![] } else { aliases.split(',').map(|s| s.trim().to_string()).collect() },
                        if tags.is_empty() { vec![] } else { tags.split(',').map(|s| s.trim().to_string()).collect() },
                        if language.is_empty() { None } else { Some(language.clone()) },
                    );

                    if let Err(e) = add_project(&mut self.config, project) {
                        self.error_message = Some(format!("保存に失敗: {}", e));
                    } else {
                        let last = self.config.projects.len().saturating_sub(1);
                        self.screen = Screen::project_list_at(last);
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('\x1b') => {
                self.screen = Screen::project_list();
            }
            KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field < field_count - 1 {
                    *current_field += 1;
                }
            }
            KeyCode::Char('n') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field < field_count - 1 {
                    *current_field += 1;
                }
            }
            KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field > 0 {
                    *current_field -= 1;
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field > 0 {
                    *current_field -= 1;
                }
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.input_to_field(*current_field, c, name, path, aliases, tags, language);
            }
            KeyCode::Backspace => {
                self.backspace_field(*current_field, name, path, aliases, tags, language);
            }
            _ => {}
        }

        Ok(())
    }

    /// 編集フォーム画面のキー入力を処理
    fn handle_edit_form_key(
        &mut self,
        key: KeyEvent,
        index: usize,
        name: &mut String,
        path: &mut String,
        aliases: &mut String,
        tags: &mut String,
        language: &mut String,
        current_field: &mut usize,
    ) -> Result<()> {
        let fields = FormField::all();
        let field_count = fields.len();

        match key.code {
            KeyCode::Enter | KeyCode::Char('\r') | KeyCode::Char('\n') => {
                if *current_field < field_count - 1 {
                    // 次のフィールドへ
                    *current_field += 1;
                    return Ok(());
                } else {
                    // 保存
                    if name.is_empty() || path.is_empty() {
                        self.error_message = Some("名前とパスは必須です".to_string());
                        return Ok(());
                    }

                    // 既存IDを保持して更新
                    let existing = &self.config.projects[index];
                    let project = existing.update_from(
                        name.clone(),
                        path.clone(),
                        if aliases.is_empty() { vec![] } else { aliases.split(',').map(|s| s.trim().to_string()).collect() },
                        if tags.is_empty() { vec![] } else { tags.split(',').map(|s| s.trim().to_string()).collect() },
                        if language.is_empty() { None } else { Some(language.clone()) },
                    );

                    if let Err(e) = update_project(&mut self.config, index, project) {
                        self.error_message = Some(format!("更新に失敗: {}", e));
                    } else {
                        self.screen = Screen::project_list_at(index);
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('\x1b') => {
                self.screen = Screen::project_list();
            }
            KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field < field_count - 1 {
                    *current_field += 1;
                }
            }
            KeyCode::Char('n') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field < field_count - 1 {
                    *current_field += 1;
                }
            }
            KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field > 0 {
                    *current_field -= 1;
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if *current_field > 0 {
                    *current_field -= 1;
                }
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.input_to_field(*current_field, c, name, path, aliases, tags, language);
            }
            KeyCode::Backspace => {
                self.backspace_field(*current_field, name, path, aliases, tags, language);
            }
            _ => {}
        }

        Ok(())
    }

    /// 削除確認ダイアログのキー入力を処理
    fn handle_delete_confirm_key(&mut self, key: KeyEvent, index: usize) -> Result<()> {
        match key.code {
            KeyCode::Char('y') => {
                if let Err(e) = delete_project(&mut self.config, index) {
                    self.error_message = Some(format!("削除に失敗: {}", e));
                } else {
                    // 削除後、選択位置を有効範囲内にクランプ
                    let new_index = index.min(self.config.projects.len().saturating_sub(1));
                    self.screen = Screen::project_list_at(new_index);
                }
            }
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('\x1b') => {
                self.screen = Screen::project_list();
            }
            _ => {}
        }

        Ok(())
    }

    /// フィールドに文字を入力
    fn input_to_field(
        &self,
        field: usize,
        c: char,
        name: &mut String,
        path: &mut String,
        aliases: &mut String,
        tags: &mut String,
        language: &mut String,
    ) {
        let fields = FormField::all();

        if let Some(form_field) = fields.get(field) {
            match form_field {
                FormField::Name => name.push(c),
                FormField::Path => path.push(c),
                FormField::Aliases => aliases.push(c),
                FormField::Tags => tags.push(c),
                FormField::Language => language.push(c),
            }
        }
    }

    /// フィールドから文字を削除
    fn backspace_field(
        &self,
        field: usize,
        name: &mut String,
        path: &mut String,
        aliases: &mut String,
        tags: &mut String,
        language: &mut String,
    ) {
        let fields = FormField::all();

        if let Some(form_field) = fields.get(field) {
            match form_field {
                FormField::Name => { name.pop(); }
                FormField::Path => { path.pop(); }
                FormField::Aliases => { aliases.pop(); }
                FormField::Tags => { tags.pop(); }
                FormField::Language => { language.pop(); }
            }
        }
    }
}
