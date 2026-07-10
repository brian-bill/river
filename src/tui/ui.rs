use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::tui::app::{App, Status};
use crate::tui::highlight;
use crate::tui::output::OutputLine;
use crate::tui::theme::Theme;

pub fn compute_layout_rects(area: Rect, app: &App) -> Vec<Rect> {
    let loader_height: u16 = if matches!(app.status, Status::Running) { 1 } else { 0 };
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(input_height(app, area, loader_height)),
        ])
        .split(area)
        .to_vec()
}

pub fn render(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    let area = frame.area();

    let main_layout = compute_layout_rects(area, app);

    render_header(frame, main_layout[0], app, theme);
    render_output(frame, main_layout[1], app, theme);
    render_input(frame, main_layout[2], app, theme);
}

fn input_height(app: &App, area: Rect, loader_height: u16) -> u16 {
    let lines = app.input.line_count() as u16;
    let needed = lines.saturating_add(1).saturating_add(loader_height);
    let reserved = 6u16;
    let max = area.height.saturating_sub(reserved).max(6);
    let min = 6u16.saturating_add(loader_height);
    needed.clamp(min, max)
}

fn render_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let conn_info = app
        .active_connection.as_deref()
        .unwrap_or("no connection");

    let status_indicator = match app.status {
        Status::Running => Span::styled(
            " ⏳ Running... ",
            theme
                .header_style()
                .fg(theme.output_json_number)
                .add_modifier(Modifier::BOLD),
        ),
        Status::Error(_) => Span::styled(
            " ✗ ",
            theme.header_style().fg(theme.output_error),
        ),
        Status::Idle => Span::styled(
            " ✓ ",
            theme.header_style().fg(theme.header_fg),
        ),
    };

    let text = Line::from(vec![
        Span::styled(" River ", theme.header_style().add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" {} ", conn_info),
            theme
                .header_style()
                .fg(theme.output_dim)
                .add_modifier(Modifier::ITALIC),
        ),
        status_indicator,
    ]);

    frame.render_widget(
        Paragraph::new(text).block(Block::default().style(theme.header_style())),
        area,
    );
}

fn render_output(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let [content_area, scrollbar_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(1),
    ]).areas(area);

    let visible_height = content_area.height as usize;
    let (offset, visible_lines) = app.output.visible_range(visible_height);

    let total_content = app.output.total_visual_lines();
    let mut scrollbar_state = ScrollbarState::new(total_content)
        .viewport_content_length(visible_height)
        .position(offset);

    let lines: Vec<Line> = visible_lines
        .iter()
        .flat_map(|line| render_output_line(line, content_area.width, visible_height, theme))
        .collect();

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::NONE),
            ),
        content_area,
    );

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("█")
        .track_style(Style::default().fg(theme.scrollbar_track))
        .thumb_style(Style::default().fg(theme.scrollbar_thumb));

    frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
}

fn render_output_line<'a>(line: &'a OutputLine, max_width: u16, available_height: usize, theme: &'a Theme) -> Vec<Line<'a>> {
    match line {
        OutputLine::Info(text) => vec![Line::from(Span::styled(
            text.as_str(),
            theme.output_info_style(),
        ))],
        OutputLine::Error(text) => {
            let indicator = Span::styled(
                "✗ ",
                Style::default()
                    .fg(Color::White)
                    .bg(theme.output_error)
                    .add_modifier(Modifier::BOLD),
            );
            let msg = Span::styled(text.as_str(), theme.output_error_style());
            vec![Line::from(vec![indicator, msg])]
        }
        OutputLine::Separator => {
            let width = max_width as usize;
            vec![Line::from(Span::styled(
                "─".repeat(width),
                theme.separator_style(),
            ))]
        }
        OutputLine::Json(json) => {
            vec![Line::from(highlight_json(json, theme))]
        }
        OutputLine::Table { headers, rows, row_offset, col_offset } => {
            render_compact_table(headers, rows, *row_offset, *col_offset, max_width, available_height, theme)
        }
    }
}

fn render_compact_table<'a>(
    headers: &[String],
    rows: &[Vec<String>],
    row_offset: usize,
    col_offset: usize,
    max_width: u16,
    available_height: usize,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines: Vec<Line<'a>> = Vec::new();

    let col_count = headers.len();
    if col_count == 0 || rows.is_empty() {
        lines.push(Line::from("(empty table)"));
        return lines;
    }

    let total_rows = rows.len();
    let max_display = crate::tui::output::MAX_TABLE_DISPLAY_ROWS;
    let cap = max_display.min(available_height.saturating_sub(6));
    let cap = cap.max(1);

    let start = row_offset.min(total_rows.saturating_sub(1));
    let end = (start + cap).min(total_rows);
    let slice = &rows[start..end];

    let has_more_above = start > 0;
    let has_more_below = end < total_rows;

    let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in slice {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }
    }

    let max_col = 40usize;
    for w in &mut col_widths {
        *w = (*w).min(max_col).max(3);
    }

    let cell_padding: usize = 3;
    let total_content_width: usize = col_widths.iter().sum::<usize>() + (col_widths.len().saturating_sub(1)) * cell_padding + 2;
    let viewport_width = max_width.saturating_sub(1) as usize;

    let (visible_cols_start, visible_cols_end) = if total_content_width <= viewport_width {
        (0_usize, col_count)
    } else {
        let first_visible = col_offset.min(col_count.saturating_sub(1));

        let mut last_visible = first_visible;
        let mut w_accum = 1usize;
        for (i, &col_width) in col_widths.iter().enumerate().skip(first_visible) {
            let col_total = col_width + cell_padding;
            if w_accum + col_total > viewport_width && i > first_visible {
                break;
            }
            w_accum += col_total;
            last_visible = i + 1;
        }
        (first_visible, last_visible.min(col_count))
    };

    let visible_headers: Vec<&str> = headers[visible_cols_start..visible_cols_end].iter().map(|s| s.as_str()).collect();
    let visible_widths: Vec<usize> = col_widths[visible_cols_start..visible_cols_end].to_vec();

    let effective: usize = visible_widths.iter().sum();
    let bar_width = effective + visible_widths.len().saturating_mul(cell_padding).saturating_sub(1);
    let bar_width_clamped = bar_width.min(viewport_width).max(3);

    let h_style = theme.table_header_style();
    let border_fg = theme.table_border;

    let bar = |c1: char, c2: char| -> Line<'a> {
        Line::from(Span::styled(
            format!("{}{}{}", c1, "─".repeat(bar_width_clamped), c2),
            Style::default().fg(border_fg),
        ))
    };

    if has_more_above {
        lines.push(Line::from(Span::styled(
            format!("  ... {} more row(s) above (press Up to scroll) ...", start),
            theme.output_info_style(),
        )));
    }

    lines.push(bar('┌', '┐'));

    let mut header_spans: Vec<Span> = vec![Span::styled("│ ", Style::default().fg(border_fg))];
    for (i, h) in visible_headers.iter().enumerate() {
        let w = visible_widths[i];
        let padded = format!("{:w$}", h, w = w);
        header_spans.push(Span::styled(padded, h_style));
        if i < visible_headers.len() - 1 {
            header_spans.push(Span::styled(" │ ", Style::default().fg(border_fg)));
        }
    }
    header_spans.push(Span::styled(" │", Style::default().fg(border_fg)));
    lines.push(Line::from(header_spans));

    lines.push(bar('├', '┤'));

    for (ri, row) in slice.iter().enumerate() {
        let row_style = if ri % 2 == 0 {
            theme.output_text_style()
        } else {
            Style::default()
                .fg(theme.output_text)
                .bg(theme.table_row_alt)
        };

        let mut row_spans: Vec<Span> = vec![Span::styled("│ ", Style::default().fg(border_fg))];
        for (i, cell) in row[visible_cols_start..visible_cols_end].iter().enumerate() {
            let w = visible_widths[i];
            let padded = format!("{:w$}", cell, w = w);
            row_spans.push(Span::styled(padded, row_style));
            if i < row[visible_cols_start..visible_cols_end].len() - 1 {
                row_spans.push(Span::styled(" │ ", Style::default().fg(border_fg)));
            }
        }
        row_spans.push(Span::styled(" │", Style::default().fg(border_fg)));
        lines.push(Line::from(row_spans));
    }

    lines.push(bar('└', '┘'));

    if total_content_width > viewport_width {
        let max_col_offset = col_count.saturating_sub(1);
        lines.push(Line::from(Span::styled(
            format!("  ← col {}/{} — Shift+← → or Shift+wheel to scroll →", col_offset + 1, max_col_offset + 1),
            theme.output_info_style(),
        )));
    }

    if has_more_below {
        let remaining = total_rows - end;
        lines.push(Line::from(Span::styled(
            format!("  ... {} more row(s) below (press Down to scroll) ...", remaining),
            theme.output_info_style(),
        )));
    }

    lines
}

fn highlight_json<'a>(json: &'a str, theme: &'a Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = json.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        match c {
            '{' | '}' | '[' | ']' | ',' | ':' => {
                spans.push(Span::styled(
                    c.to_string(),
                    Style::default().fg(theme.output_json_brace),
                ));
                i += 1;
            }
            '"' => {
                let mut s = String::new();
                s.push('"');
                i += 1;
                while i < len {
                    if chars[i] == '\\' && i + 1 < len {
                        s.push(chars[i]);
                        s.push(chars[i + 1]);
                        i += 2;
                    } else if chars[i] == '"' {
                        s.push('"');
                        i += 1;
                        break;
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                let mut j = i;
                while j < len && (chars[j] == ' ' || chars[j] == '\n' || chars[j] == '\t') {
                    j += 1;
                }
                let is_key = j < len && chars[j] == ':';
                let style = if is_key {
                    Style::default().fg(theme.output_json_key)
                } else {
                    Style::default().fg(theme.output_json_string)
                };
                spans.push(Span::styled(s, style));
            }
            't' | 'f' if looks_like_bool(&chars, i) => {
                let word = if chars[i] == 't' { "true" } else { "false" };
                i += word.len();
                spans.push(Span::styled(
                    word,
                    Style::default().fg(theme.output_json_bool),
                ));
            }
            'n' if looks_like_null(&chars, i) => {
                spans.push(Span::styled(
                    "null",
                    Style::default().fg(theme.output_json_null),
                ));
                i += 4;
            }
            '-' | '0'..='9' => {
                let num: String = chars[i..]
                    .iter()
                    .take_while(|c| matches!(c, '-' | '0'..='9' | '.' | 'e' | 'E' | '+'))
                    .collect();
                let len = num.len();
                spans.push(Span::styled(
                    num,
                    Style::default().fg(theme.output_json_number),
                ));
                i += len;
            }
            _ => {
                let mut s = String::new();
                while i < len
                    && !matches!(
                        chars[i],
                        '{' | '}' | '[' | ']' | ',' | ':' | '"' | '-' | '0'..='9'
                    )
                {
                    if looks_like_bool(&chars, i) || looks_like_null(&chars, i) {
                        break;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                if !s.is_empty() {
                    spans.push(Span::styled(s, Style::default().fg(theme.output_text)));
                }
            }
        }
    }

    spans
}

fn looks_like_bool(chars: &[char], i: usize) -> bool {
    chars.get(i..).is_some_and(|s| {
        (s.len() >= 4 && s[0] == 't' && s[1] == 'r' && s[2] == 'u' && s[3] == 'e')
            || (s.len() >= 5
                && s[0] == 'f'
                && s[1] == 'a'
                && s[2] == 'l'
                && s[3] == 's'
                && s[4] == 'e')
    })
}

fn looks_like_null(chars: &[char], i: usize) -> bool {
    chars
        .get(i..)
        .is_some_and(|s| s.len() >= 4 && s[0] == 'n' && s[1] == 'u' && s[2] == 'l' && s[3] == 'l')
}

fn render_input(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let is_running = matches!(app.status, Status::Running);
    let loader_height: usize = if is_running { 1 } else { 0 };
    let border_height = 1usize;
    let area_height = (area.height as usize).saturating_sub(border_height + loader_height);

    let tokens = highlight::tokenize(&app.input.text);

    let text_width = (area.width as usize).saturating_sub(3).max(10);
    let logical_lines = app.input.lines();
    let cursor_logical_line = app.input.cursor_line();
    let cursor_logical_col = app.input.cursor_col();

    struct Chunk {
        text: String,
        byte_start: usize,
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut display_lines: Vec<(usize, usize)> = Vec::new();
    let mut cursor_display_line: Option<usize> = None;
    let mut cursor_display_col: usize = 0;

    for (li, line_text) in logical_lines.iter().enumerate() {
        let line_start = highlight::line_start_byte(&app.input.text, li);
        let chars: Vec<char> = line_text.chars().collect();
        let mut start = 0;
        while start < chars.len() || (start == 0 && chars.is_empty()) {
            let end = (start + text_width).min(chars.len());
            let chunk_text: String = chars[start..end].iter().collect();
            let byte_offset: usize = line_text
                .chars()
                .take(start)
                .map(|c| c.len_utf8())
                .sum();
            chunks.push(Chunk { text: chunk_text, byte_start: line_start + byte_offset });
            display_lines.push((li, chunks.len() - 1));

            if li == cursor_logical_line {
                let col_in_chars = cursor_logical_col;
                if col_in_chars >= start && (col_in_chars < end || end == chars.len()) {
                    cursor_display_line = Some(display_lines.len() - 1);
                    cursor_display_col = col_in_chars - start;
                }
            }

            if end >= chars.len() {
                break;
            }
            start = end;
        }
    }

    let total_display = display_lines.len();
    let cursor_dl = cursor_display_line.unwrap_or(0);

    let (view_offset, content_height) = if total_display <= area_height {
        (0, total_display)
    } else {
        let ch = area_height.saturating_sub(2).max(1);
        let off = if cursor_dl < ch / 2 {
            0
        } else if cursor_dl >= total_display.saturating_sub(ch / 2) {
            total_display.saturating_sub(ch)
        } else {
            cursor_dl.saturating_sub(ch / 2)
        };
        (off, ch)
    };

    let view_end = (view_offset + content_height).min(total_display);
    let mut rendered_lines: Vec<Line> = Vec::with_capacity(area_height);

    if view_offset > 0 {
        rendered_lines.push(Line::from(Span::styled(
            format!("  ↑ {} more line(s) above", view_offset),
            Style::default().fg(theme.output_dim),
        )));
    }

    for (di, &(_logical_idx, chunk_idx)) in display_lines.iter().enumerate().skip(view_offset).take(view_end - view_offset) {
        let chunk = &chunks[chunk_idx];

        let is_cursor_line = Some(di) == cursor_display_line;
        let cursor_byte = if is_cursor_line {
            let byte_pos: usize = chunk
                .text
                .chars()
                .take(cursor_display_col)
                .map(|c| c.len_utf8())
                .sum();
            Some(byte_pos)
        } else {
            None
        };

        let spans = highlight::highlight_line(&chunk.text, chunk.byte_start, &tokens, cursor_byte, theme);

        let prefix = if is_cursor_line {
            Span::styled("> ", theme.input_prefix_style())
        } else {
            Span::styled("  ", Style::default().fg(theme.output_dim))
        };

        let mut line_spans = vec![prefix];
        line_spans.extend(spans);
        rendered_lines.push(Line::from(line_spans));
    }

    if view_end < total_display {
        let remaining = total_display - view_end;
        rendered_lines.push(Line::from(Span::styled(
            format!("  ↓ {} more line(s) below", remaining),
            Style::default().fg(theme.output_dim),
        )));
    }

    if rendered_lines.is_empty() {
        let prefix = Span::styled("> ", theme.input_prefix_style());
        let cursor = Span::styled(" ", theme.input_cursor_style());
        rendered_lines.push(Line::from(vec![prefix, cursor]));
    }

    if is_running {
        let spinner = spinner_char();
        rendered_lines.push(Line::from(Span::styled(
            format!("  {} Running...", spinner),
            Style::default()
                .fg(theme.output_json_number)
                .add_modifier(Modifier::BOLD),
        )));
    }

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(theme.separator_style());

    frame.render_widget(
        Paragraph::new(Text::from(rendered_lines)).block(block),
        area,
    );
}

fn spinner_char() -> char {
    const CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    CHARS[(millis / 100) as usize % CHARS.len()]
}
