use std::io::{self, Write};

use crate::scanner::extract_port;
use crate::types::PortProc;

pub fn clear_screen() {
    print!("\x1b[H\x1b[2J");
    io::stdout().flush().unwrap(); // Force output immediately
}

pub fn enter_alternate_screen() {
    print!("\x1b[?1049h"); // Enter alternate screen
    std::io::stdout().flush().unwrap();
}

pub fn exit_alternate_screen() {
    print!("\x1b[?1049l"); // Exit alternate screen (restores previous content)
    std::io::stdout().flush().unwrap();
}

pub fn render_table(items: &[PortProc]) {
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
