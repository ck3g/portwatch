use clap::Parser;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

mod types;
use types::PortProc;

mod parser;
use parser::{extract_port, fetch_ss_output, scan_ports};

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
    } else {
        println!("Running default");
    }
}

fn handle_once() {
    match fetch_ss_output() {
        Ok(stdout) => {
            let mut pp = scan_ports(stdout);
            pp.sort_by_key(|p| extract_port(&p.local_address));

            render_table(&pp);
        }
        Err(err) => eprintln!("scan failed: {err}"),
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

fn clear_screen() {
    print!("\x1b[H\x1b[2J");
    io::stdout().flush().unwrap(); // Force output immediately
}

fn enter_alternate_screen() {
    print!("\x1b[?1049h"); // Enter alternate screen
    std::io::stdout().flush().unwrap();
}

fn exit_alternate_screen() {
    print!("\x1b[?1049l"); // Exit alternate screen (restores previous content)
    std::io::stdout().flush().unwrap();
}

fn render_table(items: &[PortProc]) {
    if items.is_empty() {
        println!("No listening ports found.");
        return;
    }

    let reverse = "\x1b[7m"; // reverse colors (swap background and foreground)
    let reset = "\x1b[0m"; // reset to normal

    let max_proc_len = items
        .iter()
        .map(|p| p.proc_name.len())
        .max()
        .unwrap_or(7)
        .max(7);

    println!(
        "{}{:<6} {:>6} {:<10} {:>20} {:<8} {:<width$}{}",
        reverse, //start background
        "PROTO",
        "PID",
        "STATE",
        "ADDRESS",
        "PORT",
        "PROCESS",
        reset, //end background
        width = max_proc_len
    );

    for item in items {
        let state_str = item.state.as_deref().unwrap_or("");
        let port_str = extract_port(&item.local_address).map_or("?".to_string(), |p| p.to_string());
        let local_address = item
            .local_address
            .rsplit_once(":")
            .map(|(addr, _)| addr)
            .unwrap_or(&item.local_address);
        println!(
            "{:<6} {:>6} {:<10} {:>20} {:<8} {:<width$}",
            item.proto,
            item.pid,
            state_str,
            local_address,
            port_str,
            item.proc_name,
            width = max_proc_len
        );
    }
}
