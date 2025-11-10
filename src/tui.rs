use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::Duration;

pub fn run_tui() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(render)?;

        // Poll with timeout (non-blocking)
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.code == KeyCode::Char('q') => break,
                _ => {}
            }
        }
    }

    Ok(())
}

fn render(frame: &mut Frame) {
    let block = Block::default()
        .title("Portwatch TUI")
        .borders(Borders::ALL);
    let paragraph = Paragraph::new("Hello TUI! Press 'q' to quit.").block(block);
    frame.render_widget(paragraph, frame.area());
}
