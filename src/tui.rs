use crate::{
    parser::{extract_port, fetch_ss_output, scan_ports},
    types::PortProc,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use std::time::{Duration, Instant};

const REFRESH_INTERVAL_SEC: u64 = 5;
const POLL_TIMEOUT_MS: u64 = 100;
const HIGHLIGHT_BG_COLOR: Color = Color::Indexed(236);
const HIGHLIGHT_SYMBOL: &str = "→ ";
const TITLE: &str = " PortWatch ";
const AVAILABLE_SIGNALS: [(Signal, &str, &str); 2] = [
    (Signal::SIGTERM, "SIGTERM (15)", "Graceful shutdown"),
    (Signal::SIGKILL, "SIGKILL (9)", "Force kill"),
];

enum Mode {
    Normal,
    SignalSelect { signal_index: usize },
    Filter,
}

struct App {
    items: Vec<PortProc>,
    selected: usize,
    mode: Mode,
    filter: String,
}

impl App {
    fn new(items: Vec<PortProc>) -> Self {
        Self {
            items,
            selected: 0,
            mode: Mode::Normal,
            filter: String::new(),
        }
    }

    fn next(&mut self) {
        let filtered = self.filtered_items();
        if filtered.is_empty() {
            return;
        }

        self.selected = if self.selected == filtered.len() - 1 {
            0
        } else {
            self.selected + 1
        }
    }

    fn previous(&mut self) {
        let filtered = self.filtered_items();
        if filtered.is_empty() {
            return;
        }

        self.selected = if self.selected == 0 {
            filtered.len() - 1
        } else {
            self.selected - 1
        }
    }

    fn refresh_items(&mut self, new_items: Vec<PortProc>) {
        self.items = new_items;

        // Clamp to filtered list if filter is active
        let filtered_len = self.filtered_items().len();
        if !filtered_len == 0 && self.selected >= filtered_len {
            self.selected = filtered_len - 1;
        } else if !self.items.is_empty() && self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
    }

    fn filtered_items(&self) -> Vec<&PortProc> {
        if self.filter.is_empty() {
            return self.items.iter().collect();
        }
        let filter_text = self.filter.to_lowercase();

        // Filter items that match the search text
        self.items
            .iter()
            .filter(|item| {
                // Match against process name
                item.proc_name.to_lowercase().contains(&filter_text)
                    // Match against PID
                    || item.pid.to_string().contains(&filter_text)
                    // Match against port
                    || extract_port(&item.local_address)
                        .map(|p| p.to_string().contains(&filter_text))
                        .unwrap_or(false)
                    // Match against address
                    || item.local_address.to_lowercase().contains(&filter_text)
                    // Match against protocol
                    || item.proto.to_string().to_lowercase().contains(&filter_text)
            })
            .collect()
    }
}

fn send_signal(pid: u32, signal: Signal) -> Result<()> {
    kill(Pid::from_raw(pid as i32), signal)?;
    Ok(())
}

pub fn run_tui() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    let output = fetch_ss_output()?;
    let mut items = scan_ports(output);
    items.sort_by_key(|p| extract_port(&p.local_address));

    let mut app = App::new(items);
    let mut last_refresh = Instant::now();
    let refresh_interval = Duration::from_secs(REFRESH_INTERVAL_SEC);

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        // Poll with timeout (non-blocking)
        if event::poll(Duration::from_millis(POLL_TIMEOUT_MS))?
            && let Event::Key(key) = event::read()?
        {
            match app.mode {
                Mode::Normal => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('s') => {
                        app.mode = Mode::SignalSelect { signal_index: 0 };
                    }
                    KeyCode::Char('/') => {
                        app.mode = Mode::Filter;
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    _ => {}
                },
                Mode::SignalSelect { .. } => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Normal;
                    }

                    KeyCode::Up | KeyCode::Char('k') => {
                        if let Mode::SignalSelect { signal_index } = &mut app.mode {
                            *signal_index = if *signal_index == 0 { 1 } else { 0 };
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Mode::SignalSelect { signal_index } = &mut app.mode {
                            *signal_index = if *signal_index == 0 { 1 } else { 0 };
                        }
                    }
                    KeyCode::Enter => {
                        if let Mode::SignalSelect { signal_index } = app.mode {
                            let filtered = app.filtered_items();
                            if !filtered.is_empty() {
                                let selected_item = filtered[app.selected];
                                let (signal, _, _) = AVAILABLE_SIGNALS[signal_index];

                                if let Err(e) = send_signal(selected_item.pid, signal) {
                                    eprintln!("Failed to send signal: {}", e);
                                }

                                // Refresh immediately to show the killed process is gone
                                if let Ok(output) = fetch_ss_output() {
                                    let mut new_items = scan_ports(output);
                                    new_items.sort_by_key(|p| extract_port(&p.local_address));
                                    app.refresh_items(new_items);
                                }
                            }
                        }

                        app.mode = Mode::Normal;
                    }
                    _ => {}
                },
                Mode::Filter => match key.code {
                    KeyCode::Esc => {
                        app.filter.clear();
                        app.selected = 0;
                        app.mode = Mode::Normal;
                    }
                    KeyCode::Enter => {
                        app.mode = Mode::Normal;
                    }
                    KeyCode::Backspace => {
                        app.filter.pop();
                        app.selected = 0;
                    }
                    KeyCode::Char(c) => {
                        app.filter.push(c);
                        app.selected = 0;
                    }
                    _ => {}
                },
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            let output = fetch_ss_output()?;
            let mut new_items = scan_ports(output);
            new_items.sort_by_key(|p| extract_port(&p.local_address));
            app.refresh_items(new_items);
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let filtered = app.filtered_items();
    let total_count = app.items.len();
    let filtered_count = filtered.len();

    let header_style = Style::default().add_modifier(Modifier::REVERSED);
    let header = Row::new(vec![
        Cell::from("PROTO"),
        Cell::from(Text::from("PID").alignment(Alignment::Right)),
        Cell::from("STATE"),
        Cell::from(Text::from("ADDRESS").alignment(Alignment::Right)),
        Cell::from("PORT"),
        Cell::from("PROCESS"),
    ])
    .style(header_style)
    .height(1);
    let rows: Vec<Row> = filtered
        .iter()
        .map(|item| {
            let port = extract_port(&item.local_address).map_or("?".to_string(), |p| p.to_string());
            let state = item.state.as_deref().unwrap_or("");
            let address = item
                .local_address
                .rsplit_once(':')
                .map(|(addr, _)| addr)
                .unwrap_or(&item.local_address);

            Row::new(vec![
                Cell::from(item.proto.to_string()),
                Cell::from(Text::from(item.pid.to_string()).alignment(Alignment::Right)),
                Cell::from(state.to_string()),
                Cell::from(Text::from(address.to_string()).alignment(Alignment::Right)),
                Cell::from(port),
                Cell::from(item.proc_name.to_string()),
            ])
        })
        .collect();

    let total_title = if filtered_count < total_count {
        format!("Showing {} of {} processes", filtered_count, total_count)
    } else {
        format!("{} processes", total_count)
    };

    let title_bottom = format!(
        " {} | ↑↓ or j/k: navigate | s: signal | /: filter | q: quit ",
        total_title
    );

    let table = Table::new(
        rows,
        vec![
            Constraint::Length(6),      // PROTO
            Constraint::Length(6),      // PID
            Constraint::Min(10),        // STATE
            Constraint::Percentage(30), // ADDRESS
            Constraint::Length(8),      // PORT
            Constraint::Percentage(40), // PROCESS
        ],
    )
    .header(header)
    .row_highlight_style(Style::default().bg(HIGHLIGHT_BG_COLOR))
    .highlight_symbol(HIGHLIGHT_SYMBOL)
    .block(
        Block::default()
            .title_top(Line::from(TITLE).centered())
            .title_bottom(Line::from(title_bottom).centered())
            .borders(Borders::ALL),
    );

    let mut table_state = TableState::default().with_selected(Some(app.selected));

    frame.render_stateful_widget(table, frame.area(), &mut table_state);

    if let Mode::SignalSelect { signal_index } = app.mode {
        render_signal_modal(frame, app, signal_index);
    }

    if let Mode::Filter = &app.mode {
        render_filter_input(frame, &app.filter);
    }
}

fn render_signal_modal(frame: &mut Frame, app: &App, selected_signal: usize) {
    let filtered = app.filtered_items();
    if filtered.is_empty() {
        return;
    }

    let process = filtered[app.selected];

    let area = centered_rect(50, 40, frame.area());

    let rows: Vec<Row> = AVAILABLE_SIGNALS
        .iter()
        .map(|(_, name, desc)| Row::new(vec![Cell::from(format!("{} - {}", name, desc))]))
        .collect();

    let title = format!(
        " Send signal to PID {} ({}) ",
        process.pid, process.proc_name
    );
    let hint = " ↵: Send | Esc: Cancel | ↑↓ or j/k: Select ".to_string();

    let block = Block::default()
        .title_top(Line::from(title).centered())
        .title_bottom(Line::from(hint).centered())
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let mut table_state = TableState::default().with_selected(selected_signal);

    let table = Table::new(rows, vec![Constraint::Percentage(100)])
        .row_highlight_style(Style::default().bg(HIGHLIGHT_BG_COLOR))
        .highlight_symbol(HIGHLIGHT_SYMBOL)
        .block(block);

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_filter_input(frame: &mut Frame, input: &str) {
    // Split screen to reserve bottom line for filter input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(frame.area());

    let hint = " ⏎ Apply | Esc Cancel ";

    let block = Block::default()
        .title_top(Line::from(" Filter ").left_aligned())
        .title_bottom(Line::from(hint).centered())
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(format!("{}{}", HIGHLIGHT_SYMBOL, input)).block(block);

    frame.render_widget(paragraph, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PortProc, Proto};

    fn mock_port_proc() -> PortProc {
        PortProc {
            proto: Proto::Tcp,
            local_address: "127.0.0.1:3000".to_string(),
            pid: 1234,
            proc_name: "test".to_string(),
            state: Some("LISTEN".to_string()),
        }
    }

    #[test]
    fn test_next_empty_list() {
        let mut app = App::new(vec![]);
        app.next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_next() {
        let mut app = App::new(vec![mock_port_proc(), mock_port_proc()]);
        app.next();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_next_wraps_around() {
        let mut app = App::new(vec![mock_port_proc(), mock_port_proc()]);
        app.selected = 1;
        app.next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_previous_empty_list() {
        let mut app = App::new(vec![]);
        app.previous();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_previous() {
        let mut app = App::new(vec![mock_port_proc(), mock_port_proc()]);
        app.selected = 1;
        app.previous();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_previous_wraps_around() {
        let mut app = App::new(vec![mock_port_proc(), mock_port_proc()]);
        app.previous();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_refresh_items() {
        let mut app = App::new(vec![mock_port_proc(), mock_port_proc(), mock_port_proc()]);
        app.selected = 2;
        let new_items: Vec<PortProc> = vec![mock_port_proc(), mock_port_proc()];
        app.refresh_items(new_items);
        assert_eq!(app.items.len(), 2);
        assert_eq!(app.selected, 1);
    }
}
