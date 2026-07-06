use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use crate::adapters::{DatabaseAdapter, Value};
use crate::ai::AiClient;
use crate::connection::{AiConfig, DatabaseKind};
use crate::engine::planner;
use crate::engine::executor;
use crate::lang::{self, ast::Statement};
use crate::tui::input::InputState;
use crate::tui::output::{OutputBuffer, OutputLine};
use crate::tui::theme::Theme;
use crate::tui::ui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Idle,
    Running,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct QuerySession {
    pub query: String,
    pub lines: Vec<OutputLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DragTarget {
    None,
    OutputVertical,
    TableHorizontal,
}

pub struct App {
    pub adapters: HashMap<String, Box<dyn DatabaseAdapter>>,
    pub source_db: Vec<(String, DatabaseKind)>,
    pub ai_configs: HashMap<String, AiConfig>,
    pub ai_client: AiClient,
    pub input: InputState,
    pub output: OutputBuffer,
    pub status: Status,
    pub quit: bool,
    pub theme: Theme,
    pub active_connection: Option<String>,
    pending_query: Option<String>,
    pub params: HashMap<String, Value>,
    pub sessions: Vec<QuerySession>,
    pub session_idx: usize,
    pub scrollbar_dragging: bool,
    pub drag_target: DragTarget,
    pub drag_origin_row: u16,
    pub drag_start_offset: usize,
    pub output_area: Rect,
    pub input_area: Rect,
}

const MAX_HISTORY: usize = 1000;

impl App {
    pub fn new(
        adapters: HashMap<String, Box<dyn DatabaseAdapter>>,
        source_db: Vec<(String, DatabaseKind)>,
        ai_configs: HashMap<String, AiConfig>,
        ai_client: AiClient,
        conn_errors: Vec<String>,
    ) -> Self {
        let mut input = InputState::new();

        load_history(&mut input);

        let active_connection = source_db.first().map(|(name, _)| name.clone());

        let mut output = OutputBuffer::new(10_000);

        output.push(OutputLine::Info(
            "River v0.10.0 — Unified Database Access".into(),
        ));

        let connected: Vec<&str> = adapters.keys().map(|s| s.as_str()).collect();
        if !connected.is_empty() {
            output.push(OutputLine::Info(format!(
                "Connected to: {}",
                connected.join(", ")
            )));
        }

        for err in &conn_errors {
            output.push(OutputLine::Error(err.clone()));
        }

        output.push(OutputLine::Separator);

        let welcome_lines = output.snapshot();
        let sessions = vec![QuerySession {
            query: String::new(),
            lines: welcome_lines,
        }];

        Self {
            adapters,
            source_db,
            ai_configs,
            ai_client,
            input,
            output,
            status: Status::Idle,
            quit: false,
            theme: Theme::default(),
            active_connection,
            pending_query: None,
            params: HashMap::new(),
            sessions,
            session_idx: 0,
            scrollbar_dragging: false,
            drag_target: DragTarget::None,
            drag_origin_row: 0,
            drag_start_offset: 0,
            output_area: Rect::default(),
            input_area: Rect::default(),
        }
    }

    fn maybe_exit_history_browse(&mut self) {
        let latest = self.sessions.len().saturating_sub(1);
        if self.session_idx != latest {
            self.snapshot_session();
            self.session_idx = latest;
            let lines = self.sessions[latest].lines.clone();
            self.output.replace_with(lines);
            self.input.reset_history_nav();
        }
    }

    fn snapshot_session(&mut self) {
        if self.session_idx < self.sessions.len() {
            self.sessions[self.session_idx].lines = self.output.snapshot();
        }
    }

    fn load_session(&mut self, idx: usize) {
        if idx < self.sessions.len() {
            let session = &self.sessions[idx];
            self.output.replace_with(session.lines.clone());
            self.input.text = session.query.clone();
            self.input.reset_history_nav();
            self.input.move_cursor_end();
        }
    }

    fn navigate_history(&mut self, delta: isize) {
        if self.sessions.is_empty() {
            return;
        }
        let new_idx = if delta < 0 {
            self.session_idx.saturating_sub(delta.unsigned_abs())
        } else {
            let add = delta as usize;
            let max = self.sessions.len().saturating_sub(1);
            (self.session_idx + add).min(max)
        };

        if new_idx != self.session_idx {
            self.snapshot_session();
            self.session_idx = new_idx;
            self.load_session(self.session_idx);
        }
    }
}

pub async fn run_event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let rects = ui::compute_layout_rects(area, app);
        app.output_area = rects[1];
        app.input_area = rects[2];

        if app.quit {
            break;
        }

        if !event::poll(Duration::from_millis(100))? {
            if let Some(query) = app.pending_query.take() {
                app.status = Status::Running;
                terminal.draw(|frame| ui::render(frame, app))?;
                execute_query(app, query).await;
                app.snapshot_session();
            }
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Release => {}
            Event::Key(key) => {
                handle_key(app, key);
            }
            Event::Paste(text) => {
                app.maybe_exit_history_browse();
                app.input.insert_text(&text);
            }
            Event::Mouse(mouse) => {
                handle_mouse(app, mouse);
            }
            Event::Resize(_, _) => {}
            _ => {}
        }

        if let Some(query) = app.pending_query.take() {
            app.status = Status::Running;
            terminal.draw(|frame| ui::render(frame, app))?;
            execute_query(app, query).await;
            app.snapshot_session();
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, key: event::KeyEvent) {
    let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Enter => {
            if has_shift || has_alt {
                app.maybe_exit_history_browse();
                app.input.insert_newline();
            } else {
                let cmd = app.input.submit();
                if !cmd.trim().is_empty() {
                    app.snapshot_session();
                    let new_session = QuerySession {
                        query: cmd.clone(),
                        lines: Vec::new(),
                    };
                    app.sessions.push(new_session);
                    app.session_idx = app.sessions.len() - 1;
                    app.output.clear();
                    for (i, line) in cmd.lines().enumerate() {
                        let prefix = if i == 0 { "> " } else { "  " };
                        app.output
                            .push(OutputLine::Info(format!("{}{}", prefix, line)));
                    }
                    save_history(&app.input);
                    app.pending_query = Some(cmd);
                }
            }
        }
        KeyCode::Char('j') if has_ctrl && !has_shift && !has_alt => {
            app.maybe_exit_history_browse();
            app.input.insert_newline();
        }
        KeyCode::Char('m') if has_ctrl && !has_shift && !has_alt => {
            app.maybe_exit_history_browse();
            app.input.insert_newline();
        }
        KeyCode::Char('d') if has_ctrl => {
            app.quit = true;
        }
        KeyCode::Char('l') if has_ctrl => {
            app.output.clear();
            app.output.push(OutputLine::Separator);
            app.snapshot_session();
        }
        KeyCode::Char('c') if has_ctrl => {
            app.status = Status::Idle;
            app.pending_query = None;
            app.output
                .push(OutputLine::Info("Query cancelled.".into()));
        }
        KeyCode::Char('w') if has_ctrl => {
            app.maybe_exit_history_browse();
            app.input.delete_word_before();
        }
        KeyCode::Up => {
            if has_ctrl {
                app.navigate_history(-1);
            } else if has_shift {
                app.output.scroll_last_table_up(5);
            } else {
                app.input.move_cursor_up();
            }
        }
        KeyCode::Down => {
            if has_ctrl {
                app.navigate_history(1);
            } else if has_shift {
                app.output.scroll_last_table_down(5);
            } else {
                app.input.move_cursor_down();
            }
        }
        KeyCode::PageUp => {
            app.output.scroll_page_up(10);
        }
        KeyCode::PageDown => {
            app.output.scroll_page_down(10);
        }
        KeyCode::Char('p') if has_ctrl => {
            app.input.history_prev();
        }
        KeyCode::Char('n') if has_ctrl => {
            app.input.history_next();
        }
        KeyCode::Left => {
            if key.modifiers.is_empty() {
                app.input.move_cursor_left();
            } else if has_shift {
                app.output.scroll_last_table_horizontally(-1);
            }
        }
        KeyCode::Right => {
            if key.modifiers.is_empty() {
                app.input.move_cursor_right();
            } else if has_shift {
                app.output.scroll_last_table_horizontally(1);
            }
        }
        KeyCode::Home => {
            app.input.move_cursor_home();
        }
        KeyCode::End => {
            app.input.move_cursor_end();
        }
        KeyCode::Backspace => {
            app.maybe_exit_history_browse();
            app.input.delete_before_cursor();
        }
        KeyCode::Delete => {
            app.maybe_exit_history_browse();
            app.input.delete_at_cursor();
        }
        KeyCode::Tab => {}
        KeyCode::Char(c) => {
            app.maybe_exit_history_browse();
            app.input.insert_char(c);
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let col = mouse.column;
    let row = mouse.row;
    let has_shift = mouse.modifiers.contains(KeyModifiers::SHIFT);

    match mouse.kind {
        MouseEventKind::ScrollDown => {
            if is_in_area(col, row, app.output_area) {
                if has_shift {
                    app.output.scroll_last_table_horizontally(1);
                } else {
                    let before = app.output.last_table_row_offset();
                    app.output.scroll_last_table_down(5);
                    let after = app.output.last_table_row_offset();
                    if before == after {
                        app.output.scroll_down(3);
                    }
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if is_in_area(col, row, app.output_area) {
                if has_shift {
                    app.output.scroll_last_table_horizontally(-1);
                } else {
                    let before = app.output.last_table_row_offset();
                    app.output.scroll_last_table_up(5);
                    let after = app.output.last_table_row_offset();
                    if before == after {
                        app.output.scroll_up(3);
                    }
                }
            }
        }
        MouseEventKind::ScrollLeft => {
            app.output.scroll_last_table_horizontally(-5);
        }
        MouseEventKind::ScrollRight => {
            app.output.scroll_last_table_horizontally(5);
        }
        MouseEventKind::Down(_button) => {
            if is_in_area(col, row, app.output_area) {
                let output_top = app.output_area.y;
                let output_height = app.output_area.height;
                if row >= output_top && row < output_top + output_height {
                    app.scrollbar_dragging = true;
                    app.drag_target = DragTarget::OutputVertical;
                    app.drag_origin_row = row;
                    app.drag_start_offset = app.output.scroll_offset();
                }
            }
        }
        MouseEventKind::Drag(_button) => {
            if app.scrollbar_dragging {
                match app.drag_target {
                    DragTarget::OutputVertical => {
                        let row_delta = row as isize - app.drag_origin_row as isize;
                        let total = app.output.total_visual_lines();
                        let viewport = app.output_area.height as usize;
                        let max_offset = total.saturating_sub(viewport);
                        let new_offset = (app.drag_start_offset as isize + row_delta)
                            .max(0)
                            .min(max_offset as isize) as usize;
                        app.output.set_scroll_offset(new_offset);
                    }
                    DragTarget::TableHorizontal => {
                        let col_delta = row as isize - app.drag_origin_row as isize;
                        app.output.scroll_last_table_horizontally(col_delta);
                        app.drag_origin_row = row;
                    }
                    DragTarget::None => {}
                }
            }
        }
        MouseEventKind::Up(_button) => {
            app.scrollbar_dragging = false;
            app.drag_target = DragTarget::None;
        }
        _ => {}
    }
}

fn is_in_area(col: u16, row: u16, area: Rect) -> bool {
    col >= area.x
        && col < area.x + area.width
        && row >= area.y
        && row < area.y + area.height
}

async fn execute_query(app: &mut App, input: String) {
    app.status = Status::Running;

    let trimmed = input.trim();
    if trimmed == ":quit" || trimmed == "exit" {
        app.quit = true;
        app.status = Status::Idle;
        return;
    }

    let stmt = match lang::parse(trimmed) {
        Ok(s) => s,
        Err(e) => {
            app.output.push(OutputLine::Error(format!("{}", e)));
            app.status = Status::Error(format!("{}", e));
            return;
        }
    };

    if let Statement::ParamAssign { name, value } = &stmt {
        match executor::expression_to_value(value) {
            Some(v) => {
                app.params.insert(name.clone(), v);
                app.status = Status::Idle;
            }
            None => {
                let msg = format!("parameter '{name}' must be assigned a literal value");
                app.output.push(OutputLine::Error(msg.clone()));
                app.status = Status::Error(msg);
            }
        }
        return;
    }

    let resolved = if app.params.is_empty() {
        stmt
    } else {
        executor::resolve_params_in_statement(&stmt, &app.params)
    };

    match &resolved {
        Statement::Describe(desc) => {
            execute_describe(app, desc).await;
        }
        Statement::ShowTables(conn) => {
            execute_show_tables(app, conn).await;
        }
        Statement::Query(_) => {
            execute_plan(app, &resolved).await;
        }
        Statement::Insert(_) | Statement::Update(_) | Statement::Delete(_) => {
            execute_dml(app, &resolved).await;
        }
        Statement::With(w) => {
            execute_with(app, w).await;
        }
        Statement::Explain(inner) => {
            execute_explain(app, inner).await;
        }
        Statement::CreateTable(_) | Statement::CreateTableAs(_) | Statement::AlterTable(_) | Statement::DropTable(_)
        | Statement::CreateDatabase(_) | Statement::DropDatabase(_) => {
            execute_dml(app, &resolved).await;
        }
        Statement::SetOp(_) | Statement::ParamAssign { .. } | Statement::Noop => {
            app.status = Status::Idle;
        }
    }
}

async fn execute_plan(app: &mut App, stmt: &Statement) {
    let plan = planner::plan_statement(stmt, &app.source_db, &app.ai_configs);
    let timing = Instant::now();

    match executor::execute_plan(&plan, &app.adapters, &app.ai_client).await {
        Ok(result) => {
            let elapsed = timing.elapsed();
            let cross_db = planner::is_cross_db(&plan.root);
            push_result(
                app,
                &result,
                Some(TimingInfo {
                    total: elapsed,
                    sources: if cross_db {
                        let dbs = planner::find_all_databases(&plan.root);
                        dbs.iter()
                            .map(|(name, kind)| format!("{kind:?}@{name}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        String::new()
                    },
                }),
            );
        }
        Err(e) => push_error(app, e),
    }
}

async fn execute_with(app: &mut App, w: &crate::lang::ast::With) {
    let stmt = Statement::With(w.clone());
    let timing = Instant::now();
    match executor::execute_statement(&stmt, &app.source_db, &app.adapters, &app.ai_configs, &app.ai_client).await {
        Ok(result) => {
            let elapsed = timing.elapsed();
            let body_stmt = w.body.as_ref();
            let plan = planner::plan_statement(body_stmt, &app.source_db, &app.ai_configs);
            let cross_db = planner::is_cross_db(&plan.root);
            push_result(
                app,
                &result,
                Some(TimingInfo {
                    total: elapsed,
                    sources: if cross_db {
                        let dbs = planner::find_all_databases(&plan.root);
                        dbs.iter()
                            .map(|(name, kind)| format!("{kind:?}@{name}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        String::new()
                    },
                }),
            );
        }
        Err(e) => push_error(app, e),
    }
}

async fn execute_describe(app: &mut App, desc: &crate::lang::ast::Describe) {
    let db_name = desc
        .connection
        .as_deref()
        .or(app.active_connection.as_deref());

    let adapter = match db_name.and_then(|n| app.adapters.get(n)) {
        Some(a) => a,
        None => {
            app.output.push(OutputLine::Error(
                "no active connection — use @connection_name".into(),
            ));
            app.status = Status::Idle;
            return;
        }
    };

    match adapter.describe_table(&desc.table, desc.schema.as_deref()).await {
        Ok(schema) => {
            let headers = vec![
                "Column".to_string(),
                "Type".to_string(),
                "Nullable".to_string(),
                "PK".to_string(),
            ];
            let rows: Vec<Vec<String>> = schema
                .columns
                .iter()
                .map(|c| {
                    vec![
                        c.name.clone(),
                        c.data_type.clone(),
                        if c.nullable {
                            "YES".to_string()
                        } else {
                            "NO".to_string()
                        },
                        if c.is_primary_key {
                            "✓".to_string()
                        } else {
                            "".to_string()
                        },
                    ]
                })
                .collect();
            app.output.push(OutputLine::Table { headers, rows, row_offset: 0, col_offset: 0 });
            app.output.scroll_to_bottom();
            app.status = Status::Idle;
        }
        Err(e) => push_error(app, e),
    }
}

async fn execute_show_tables(app: &mut App, conn: &Option<String>) {
    let db_name = conn
        .as_deref()
        .or(app.active_connection.as_deref());

    let adapter = match db_name.and_then(|n| app.adapters.get(n)) {
        Some(a) => a,
        None => {
            // No specific DB — list from all connected adapters
            let mut all: Vec<String> = Vec::new();
            for (name, adapter) in &app.adapters {
                match adapter.list_tables(None).await {                    Ok(tables) => {
                        for t in &tables {
                            let schema = t
                                .schema
                                .as_ref()
                                .map(|s| format!("{}.", s))
                                .unwrap_or_default();
                            all.push(format!("{}{}  ({})", schema, t.name, name));
                        }
                    }
                    Err(e) => {
                        app.output
                            .push(OutputLine::Error(format!("{}: {}", name, e)));
                    }
                }
            }
            let rows: Vec<Vec<String>> = all.into_iter().map(|t| vec![t]).collect();
            app.output.push(OutputLine::Table {
                headers: vec!["Table (connection)".to_string()],
                rows,
                row_offset: 0,
                col_offset: 0,
            });
            app.output.scroll_to_bottom();
            app.status = Status::Idle;
            return;
        }
    };

    match adapter.list_tables(None).await {
        Ok(tables) => {
            let rows: Vec<Vec<String>> = tables.iter().map(|t| vec![t.name.clone()]).collect();
            app.output.push(OutputLine::Table {
                headers: vec!["Table".to_string()],
                rows,
                row_offset: 0,
                col_offset: 0,
            });
            app.output.scroll_to_bottom();
            app.status = Status::Idle;
        }
        Err(e) => push_error(app, e),
    }
}

async fn execute_dml(app: &mut App, stmt: &Statement) {
    let plan = planner::plan_statement(stmt, &app.source_db, &app.ai_configs);

    match executor::execute_plan(&plan, &app.adapters, &app.ai_client).await {
        Ok(result) => {
            app.output.push(OutputLine::Info(format!(
                "OK — {} row(s) affected in {:?}",
                result.rows_affected, result.elapsed
            )));
            if !result.rows.is_empty() {
                push_result(app, &result, None);
            } else {
                app.output.scroll_to_bottom();
                app.status = Status::Idle;
            }
        }
        Err(e) => push_error(app, e),
    }
}

async fn execute_explain(app: &mut App, inner: &Statement) {
    let plan = planner::plan_statement(inner, &app.source_db, &app.ai_configs);
    let plan_lines = planner::format_plan(&plan.root);

    app.output
        .push(OutputLine::Info("── Query Plan ──".into()));
    for line in &plan_lines {
        app.output.push(OutputLine::Info(line.clone()));
    }

    let db = planner::find_all_databases(&plan.root);
    if !db.is_empty() {
        app.output.push(OutputLine::Info(format!(
            "Databases involved: {}",
            db.iter()
                .map(|(n, k)| format!("{n} ({k:?})"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    app.output.scroll_to_bottom();
    app.status = Status::Idle;
}

#[derive(Debug, Clone)]
pub struct TimingInfo {
    pub total: Duration,
    pub sources: String,
}

fn push_result(app: &mut App, result: &crate::adapters::QueryResult, timing: Option<TimingInfo>) {
    if result.columns.is_empty() && result.rows.is_empty() {
        let dur = timing
            .map(|t| format!(" in {:?}", t.total))
            .unwrap_or_default();
        app.output
            .push(OutputLine::Info(format!("OK — 0 rows{}", dur)));
        app.status = Status::Idle;
        return;
    }

    let headers = result.columns.clone();
    let rows: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(format_value)
                .collect()
        })
        .collect();

    app.output.push(OutputLine::Table { headers, rows, row_offset: 0, col_offset: 0 });

    let row_count = result.rows.len();
    let mut meta = format!("{} row(s) in {:?}", row_count, result.elapsed);

    if let Some(t) = &timing
        && !t.sources.is_empty() {
            meta.push_str(&format!(" [sources: {}]", t.sources));
        }

    app.output.push(OutputLine::Info(meta));
    app.output.scroll_to_bottom();
    app.status = Status::Idle;
}

fn push_error(app: &mut App, e: impl std::fmt::Display) {
    app.output.push(OutputLine::Error(format!("{}", e)));
    app.status = Status::Error(format!("{}", e));
}

fn format_value(v: &crate::adapters::Value) -> String {
    match v {
        crate::adapters::Value::Null => "NULL".to_string(),
        crate::adapters::Value::String(s) => s.clone(),
        crate::adapters::Value::Int(i) => i.to_string(),
        crate::adapters::Value::Float(f) => f.to_string(),
        crate::adapters::Value::Bool(b) => b.to_string(),
    }
}

fn load_history(input: &mut InputState) {
    let path = history_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        let mut seen = std::collections::HashSet::new();
        for line in content.lines() {
            let entry = if line.starts_with('"') {
                serde_json::from_str::<String>(line).unwrap_or_else(|_| line.to_string())
            } else {
                line.to_string()
            };
            let trimmed = entry.trim();
            if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
                input.history.push(entry);
            }
        }
        input.history.reverse();
        if input.history.len() > MAX_HISTORY {
            input.history.drain(0..input.history.len() - MAX_HISTORY);
        }
    }
}

fn save_history(input: &InputState) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut seen = std::collections::HashSet::new();
    let mut lines: Vec<String> = Vec::new();
    for entry in input.history.iter().rev() {
        if seen.insert(entry.as_str())
            && let Ok(json) = serde_json::to_string(entry) {
                lines.push(json);
            }
    }
    lines.reverse();
    if lines.len() > MAX_HISTORY {
        lines.drain(MAX_HISTORY..);
    }

    let content = lines.join("\n");
    let _ = std::fs::write(&path, content);
}

fn history_path() -> std::path::PathBuf {
    let base = dirs();
    base.join("history.txt")
}

fn dirs() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("river");
        return p;
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = std::path::PathBuf::from(home)
            .join("AppData")
            .join("Local")
            .join("river");
        return p;
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(".river");
    }
    std::path::PathBuf::from(".river")
}
