use crate::{
    parser::{extract_port, fetch_ss_output, scan_ports},
    types::PortProc,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Row, Table},
    Frame,
};
use std::time::{Duration, Instant};

struct App {
    items: Vec<PortProc>,
    #[allow(dead_code)]
    selected: usize,
}

pub fn run_tui() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    let output = fetch_ss_output()?;
    let items = scan_ports(output);

    let mut app = App { items, selected: 0 };
    let mut last_refresh = Instant::now();
    let refresh_interval = Duration::from_secs(5);

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        // Poll with timeout (non-blocking)
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.code == KeyCode::Char('q') => break,
                _ => {}
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            let output = fetch_ss_output()?;
            app.items = scan_ports(output);
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let header_style = Style::default().fg(Color::Black).bg(Color::White);
    let header = Row::new(vec!["PROTO", "PID", "STATE", "ADDRESS", "PORT", "PROCESS"])
        .style(header_style)
        .height(1);
    let rows: Vec<Row> = app
        .items
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
                item.proto.to_string(),
                item.pid.to_string(),
                state.to_string(),
                address.to_string(),
                port,
                item.proc_name.to_string(),
            ])
        })
        .collect();

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
    .block(
        Block::default()
            .title_top(Line::from("PortWatch").centered())
            .title_bottom(Line::from("Press 'q' to quit").centered())
            .borders(Borders::ALL),
    );

    frame.render_widget(table, frame.area());
}
