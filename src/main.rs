mod display;
mod scanner;
mod tui;
mod types;

use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use display::{clear_screen, enter_alternate_screen, exit_alternate_screen, render_table};
use scanner::create_scanner;

struct CleanupGuard;

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        exit_alternate_screen();
        handle_once();
    }
}

#[derive(Parser, Debug)]
#[command(version, about = "Monitor which processes are bound to which ports", long_about = None)]
struct Args {
    #[arg(
        long,
        help = "Run once and print port table, then exit",
        conflicts_with = "interval"
    )]
    once: bool,

    #[arg(
        long,
        help = "Run and refresh with a specified interval in seconds",
        value_parser= clap::value_parser!(u32).range(1..60*60*24),
        conflicts_with="once"
    )]
    interval: Option<u32>,
}

fn main() {
    let cli = Args::parse();
    if cli.once {
        handle_once();
    } else if let Some(interval) = cli.interval {
        handle_interval(interval);
    } else if let Err(e) = tui::run_tui() {
        eprintln!("TUI error: {}", e);
    }
}

fn handle_once() {
    match create_scanner() {
        Ok(mut scanner) => match scanner.scan() {
            Ok(items) => render_table(&items),
            Err(e) => eprintln!("Scan failed: {}", e),
        },
        Err(e) => eprintln!("Failed to create scanner: {}", e),
    }
}

fn handle_interval(interval: u32) {
    let _cleanup = CleanupGuard; // cleanup on Drop

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    enter_alternate_screen();

    while running.load(Ordering::SeqCst) {
        clear_screen();
        handle_once();
        interruptable_sleep(interval, &running);
    }
}

fn interruptable_sleep(duration_secs: u32, running: &Arc<AtomicBool>) {
    let iterations = duration_secs * 10;
    for _ in 0..iterations {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
}
