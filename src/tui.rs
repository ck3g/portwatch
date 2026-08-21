use crate::{
    scanner::{create_scanner, extract_port},
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
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};
use std::time::{Duration, Instant};

const REFRESH_INTERVAL_SEC: u64 = 5;
const STATUS_MESSAGE_TIMEOUT_SEC: u64 = 3;
const POLL_TIMEOUT_MS: u64 = 100;
const HIGHLIGHT_BG_COLOR: Color = Color::Indexed(236);
const HIGHLIGHT_SYMBOL: &str = "→ ";
const TITLE: &str = " PortWatch ";
const SIGNAL_OPTIONS: [SignalOption; 9] = [
    SignalOption::new(Signal::SIGHUP, "SIGHUP", "Hangup / reload config"),
    SignalOption::new(Signal::SIGINT, "SIGINT", "Interrupt (Ctrl+C)"),
    SignalOption::new(Signal::SIGQUIT, "SIGQUIT", "Quit with core dump"),
    SignalOption::new(Signal::SIGKILL, "SIGKILL", "Force kill"),
    SignalOption::new(Signal::SIGTERM, "SIGTERM", "Graceful shutdown"),
    SignalOption::new(Signal::SIGSTOP, "SIGSTOP", "Pause process"),
    SignalOption::new(Signal::SIGCONT, "SIGCONT", "Resume process"),
    SignalOption::new(Signal::SIGUSR1, "SIGUSR1", "User-defined signal 1"),
    SignalOption::new(Signal::SIGUSR2, "SIGUSR2", "User-defined signal 2"),
];
const HELP_TEXT: &str = "
Navigation:
    ↑/k - Move selection up
    ↓/j - Move selection down

Actions:
    s - Send signal to selected process
    q - Quit the application

Search & Filter:
    / - Start filtering (type to search)
    Esc - Clear active filter

Help:
    ? - Show/Hide this help
";

#[derive(Clone, Copy)]
struct SignalOption {
    signal: Signal,
    name: &'static str,
    description: &'static str,
}

impl SignalOption {
    const fn new(signal: Signal, name: &'static str, description: &'static str) -> Self {
        Self {
            signal,
            name,
            description,
        }
    }
}

struct PendingSignal {
    signal: Signal,
    signal_name: &'static str,
    pid: u32,
    proc_name: String,
}

#[derive(Clone, Copy)]
enum Mode {
    Normal,
    SignalSelect { signal_index: usize },
    SignalConfirm,
    Filter,
    Help { scroll: u16 },
}

struct App {
    items: Vec<PortProc>,
    selected: usize,
    mode: Mode,
    filter: String,
    status_message: Option<(String, Instant)>,
    signals: Vec<SignalOption>,
    pending_signal: Option<PendingSignal>,
}

fn available_signals() -> Vec<SignalOption> {
    let mut signals = SIGNAL_OPTIONS.to_vec();
    signals.sort_by_key(|option| option.signal as i32);
    signals
}

fn signal_label(option: &SignalOption) -> String {
    format!("{:<7} ({})", option.name, option.signal as i32)
}

impl App {
    fn new(items: Vec<PortProc>) -> Self {
        Self {
            items,
            selected: 0,
            mode: Mode::Normal,
            filter: String::new(),
            status_message: None,
            signals: available_signals(),
            pending_signal: None,
        }
    }

    fn open_signal_selection(&mut self) {
        let signal_index = self
            .signals
            .iter()
            .position(|option| option.signal == Signal::SIGTERM)
            .unwrap_or(0);
        self.mode = Mode::SignalSelect { signal_index };
    }

    fn request_signal_confirmation(&mut self) {
        let Mode::SignalSelect { signal_index } = self.mode else {
            return;
        };
        let Some(option) = self.signals.get(signal_index).copied() else {
            return;
        };
        let Some(process) = self.filtered_items().get(self.selected).copied() else {
            return;
        };

        self.pending_signal = Some(PendingSignal {
            signal: option.signal,
            signal_name: option.name,
            pid: process.pid,
            proc_name: process.proc_name.clone(),
        });
        self.mode = Mode::SignalConfirm;
    }

    fn cancel_signal_confirmation(&mut self) {
        self.pending_signal = None;
        self.mode = Mode::Normal;
    }

    fn handle_signal_selection_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Up | KeyCode::Char('k') => self.previous_signal(),
            KeyCode::Down | KeyCode::Char('j') => self.next_signal(),
            KeyCode::Enter => self.request_signal_confirmation(),
            _ => {}
        }
    }

    fn handle_signal_confirmation_key(&mut self, key: KeyCode) -> Option<PendingSignal> {
        if !matches!(self.mode, Mode::SignalConfirm) {
            return None;
        }

        match key {
            KeyCode::Char('y') => {
                self.mode = Mode::Normal;
                self.pending_signal.take()
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.cancel_signal_confirmation();
                None
            }
            _ => None,
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

    fn next_signal(&mut self) {
        if let Mode::SignalSelect { signal_index } = &mut self.mode {
            *signal_index = (*signal_index + 1) % self.signals.len();
        }
    }

    fn previous_signal(&mut self) {
        if let Mode::SignalSelect { signal_index } = &mut self.mode {
            if *signal_index == 0 {
                *signal_index = self.signals.len() - 1;
            } else {
                *signal_index -= 1;
            }
        }
    }

    fn refresh_items(&mut self, new_items: Vec<PortProc>) {
        self.items = new_items;

        // Clamp to filtered list if filter is active
        let filtered_len = self.filtered_items().len();
        if filtered_len != 0 && self.selected >= filtered_len {
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

    fn set_status(&mut self, message: String) {
        self.status_message = Some((message, Instant::now()));
    }

    fn clear_expired_status(&mut self) {
        if let Some((_, timestamp)) = &self.status_message
            && timestamp.elapsed() > Duration::from_secs(STATUS_MESSAGE_TIMEOUT_SEC)
        {
            self.status_message = None;
        }
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
    let mut scanner = create_scanner()?;
    let items = scanner.scan()?;
    let mut app = App::new(items);
    let mut last_refresh = Instant::now();
    let refresh_interval = Duration::from_secs(REFRESH_INTERVAL_SEC);

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        app.clear_expired_status();

        // Poll with timeout (non-blocking)
        if event::poll(Duration::from_millis(POLL_TIMEOUT_MS))?
            && let Event::Key(key) = event::read()?
        {
            match app.mode {
                Mode::Normal => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('?') => {
                        app.mode = Mode::Help { scroll: 0 };
                    }
                    KeyCode::Char('s') => {
                        app.open_signal_selection();
                    }
                    KeyCode::Char('/') => {
                        app.mode = Mode::Filter;
                    }
                    KeyCode::Esc => {
                        if !app.filter.is_empty() {
                            app.filter.clear();
                            app.selected = 0;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    _ => {}
                },
                Mode::SignalSelect { .. } => app.handle_signal_selection_key(key.code),
                Mode::SignalConfirm => {
                    if let Some(pending) = app.handle_signal_confirmation_key(key.code) {
                        let signal_name =
                            format!("{} ({})", pending.signal_name, pending.signal as i32);
                        match send_signal(pending.pid, pending.signal) {
                            Ok(_) => app.set_status(format!(
                                "Sent {} to PID {} ({})",
                                signal_name, pending.pid, pending.proc_name
                            )),
                            Err(e) => app.set_status(format!(
                                "Failed to send signal to PID {}: {}",
                                pending.pid, e
                            )),
                        }

                        let new_items = scanner.scan()?;
                        app.refresh_items(new_items);
                    }
                }
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
                Mode::Help { .. } => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Normal;
                    }
                    KeyCode::Char('?') => {
                        app.mode = Mode::Normal;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if let Mode::Help { scroll } = &mut app.mode
                            && *scroll > 0
                        {
                            *scroll -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Mode::Help { scroll } = &mut app.mode {
                            let max_scroll = HELP_TEXT.lines().count().saturating_sub(5) as u16;

                            if *scroll < max_scroll {
                                *scroll += 1;
                            }
                        }
                    }
                    _ => {}
                },
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            let new_items = scanner.scan()?;
            app.refresh_items(new_items);
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let filtered = app.filtered_items();

    if filtered.is_empty() && !app.filter.is_empty() {
        let message = format!(
            "No processes match filter '{}'\nPress Esc to clear filter",
            app.filter
        );
        render_message(frame, app, &message);
    } else if app.items.is_empty() {
        let message = "No processes listening on ports";
        render_message(frame, app, message);
    } else {
        render_proc_table(frame, app);
    }

    if let Mode::SignalSelect { signal_index } = app.mode {
        render_signal_modal(frame, app, signal_index);
    }

    if let Mode::SignalConfirm = app.mode {
        render_signal_confirmation_modal(frame, app);
    }

    if let Mode::Filter = &app.mode {
        render_filter_input(frame, &app.filter);
    }

    if let Mode::Help { scroll } = app.mode {
        render_help_modal(frame, scroll);
    }
}

fn render_message(frame: &mut Frame, app: &App, message: &str) {
    let hint = hint(app);
    let block = Block::default()
        .title_top(Line::from(TITLE).centered())
        .title_bottom(Line::from(hint).centered())
        .borders(Borders::ALL)
        .style(Style::default());

    let binding = format!("\n\n{}", message);
    let lines: Vec<Line> = binding
        .split("\n")
        .map(|line| Line::from(line).centered())
        .collect();

    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(paragraph, frame.area());
}

fn render_proc_table(frame: &mut Frame, app: &App) {
    let filtered = app.filtered_items();

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

    let title_bottom = hint(app);

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
}

fn render_signal_modal(frame: &mut Frame, app: &App, selected_signal: usize) {
    let filtered = app.filtered_items();
    if filtered.is_empty() {
        return;
    }

    let process = filtered[app.selected];

    let area = centered_rect(50, 40, frame.area());

    frame.render_widget(Clear, area);

    let rows: Vec<Row> = app
        .signals
        .iter()
        .map(|option| {
            Row::new(vec![Cell::from(format!(
                "{} - {}",
                signal_label(option),
                option.description
            ))])
        })
        .collect();

    let title = format!(
        " Send signal to PID {} ({}) ",
        process.pid, process.proc_name
    );
    let hint = " ↵: Review | Esc: Cancel | ↑↓ or j/k: Select ".to_string();

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

fn render_signal_confirmation_modal(frame: &mut Frame, app: &App) {
    let Some(pending) = &app.pending_signal else {
        return;
    };
    let area = centered_rect(60, 25, frame.area());
    frame.render_widget(Clear, area);

    let signal_name = format!("{} ({})", pending.signal_name, pending.signal as i32);
    let message = format!(
        "Send {} to PID {} ({})?",
        signal_name, pending.pid, pending.proc_name
    );
    let block = Block::default()
        .title_top(Line::from(" Confirm signal ").centered())
        .title_bottom(Line::from(" y: Confirm | n/Esc: Cancel ").centered())
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    let paragraph = Paragraph::new(message)
        .alignment(Alignment::Center)
        .block(block);

    frame.render_widget(paragraph, area);
}

fn render_help_modal(frame: &mut Frame, scroll_offset: u16) {
    let area = centered_rect(50, 60, frame.area());

    frame.render_widget(Clear, area);

    let title = " Help ".to_string();
    let hint = " ↑↓ or j/k: Scroll | Esc or ?: Cancel ".to_string();

    let block = Block::default()
        .title_top(Line::from(title).centered())
        .title_bottom(Line::from(hint).centered())
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(HELP_TEXT)
        .block(block)
        .scroll((scroll_offset, 0));

    frame.render_widget(paragraph, area);
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

fn hint(app: &App) -> String {
    if let Some((message, _)) = &app.status_message {
        return format!(" {} ", message);
    }

    let filtered = app.filtered_items();
    let total_count = app.items.len();
    let filtered_count = filtered.len();

    let total_title = if filtered_count < total_count {
        format!("Showing {} of {} processes", filtered_count, total_count)
    } else {
        format!("{} processes", total_count)
    };

    let title_bottom = format!(
        " {} | ↑↓ or j/k: navigate | s: signal | /: filter | q: quit ",
        total_title
    );

    title_bottom
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

    #[test]
    fn test_default_signal_index_is_sigterm() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.open_signal_selection();

        let Mode::SignalSelect { signal_index } = app.mode else {
            panic!("Expected SignalSelect mode");
        };
        assert_eq!(app.signals[signal_index].signal, Signal::SIGTERM);
    }

    #[test]
    fn test_signal_options_use_native_numbers_and_order() {
        let app = App::new(vec![]);
        let numbers: Vec<i32> = app
            .signals
            .iter()
            .map(|option| option.signal as i32)
            .collect();

        assert!(numbers.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(app.signals.iter().all(|option| {
            signal_label(option).contains(&format!("({})", option.signal as i32))
        }));
    }

    #[test]
    fn test_next_signal() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.mode = Mode::SignalSelect { signal_index: 0 };
        app.next_signal();
        if let Mode::SignalSelect { signal_index } = app.mode {
            assert_eq!(signal_index, 1);
        } else {
            panic!("Expected SignalSelect mode");
        }
    }

    #[test]
    fn test_next_signal_wraps_around() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.mode = Mode::SignalSelect {
            signal_index: app.signals.len() - 1,
        };
        app.next_signal();
        if let Mode::SignalSelect { signal_index } = app.mode {
            assert_eq!(signal_index, 0);
        } else {
            panic!("Expected SignalSelect mode");
        }
    }

    #[test]
    fn test_previous_signal() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.mode = Mode::SignalSelect { signal_index: 1 };
        app.previous_signal();
        if let Mode::SignalSelect { signal_index } = app.mode {
            assert_eq!(signal_index, 0);
        } else {
            panic!("Expected SignalSelect mode");
        }
    }

    #[test]
    fn test_previous_signal_wraps_around() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.mode = Mode::SignalSelect { signal_index: 0 };
        app.previous_signal();
        if let Mode::SignalSelect { signal_index } = app.mode {
            assert_eq!(signal_index, app.signals.len() - 1);
        } else {
            panic!("Expected SignalSelect mode");
        }
    }

    #[test]
    fn test_next_signal_noop_in_normal_mode() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.mode = Mode::Normal;
        app.next_signal();
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn test_signal_confirmation_snapshots_selected_target() {
        let mut app = App::new(vec![mock_port_proc()]);
        let signal_index = app
            .signals
            .iter()
            .position(|option| option.signal == Signal::SIGKILL)
            .unwrap();
        app.mode = Mode::SignalSelect { signal_index };

        app.request_signal_confirmation();
        app.refresh_items(vec![]);

        assert!(matches!(app.mode, Mode::SignalConfirm));
        let pending = app.pending_signal.as_ref().unwrap();
        assert_eq!(pending.signal, Signal::SIGKILL);
        assert_eq!(pending.pid, 1234);
        assert_eq!(pending.proc_name, "test");
    }

    #[test]
    fn test_enter_opens_signal_confirmation() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.open_signal_selection();

        app.handle_signal_selection_key(KeyCode::Enter);

        assert!(matches!(app.mode, Mode::SignalConfirm));
        assert!(app.pending_signal.is_some());
    }

    #[test]
    fn test_signal_confirmation_requires_y() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.open_signal_selection();
        app.request_signal_confirmation();

        assert!(app.handle_signal_confirmation_key(KeyCode::Enter).is_none());
        assert!(matches!(app.mode, Mode::SignalConfirm));

        let pending = app.handle_signal_confirmation_key(KeyCode::Char('y'));

        assert_eq!(pending.unwrap().pid, 1234);
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.pending_signal.is_none());
    }

    #[test]
    fn test_n_cancels_signal_confirmation() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.open_signal_selection();
        app.request_signal_confirmation();

        let pending = app.handle_signal_confirmation_key(KeyCode::Char('n'));

        assert!(pending.is_none());
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.pending_signal.is_none());
    }

    #[test]
    fn test_escape_cancels_signal_confirmation() {
        let mut app = App::new(vec![mock_port_proc()]);
        app.open_signal_selection();
        app.request_signal_confirmation();

        let pending = app.handle_signal_confirmation_key(KeyCode::Esc);

        assert!(pending.is_none());
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.pending_signal.is_none());
    }
}
