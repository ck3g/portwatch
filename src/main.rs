use anyhow::Context;
use clap::Parser;
use std::fmt;
use std::io::{self, Write};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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

#[derive(Debug, Clone, PartialEq)]
enum Proto {
    Tcp,
    Udp,
}

impl fmt::Display for Proto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Proto::Tcp => "TCP",
            Proto::Udp => "UDP",
        };
        f.pad(s)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct PortProc {
    proto: Proto,
    local_address: String,
    pid: u32,
    proc_name: String,
    state: Option<String>,
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

fn fetch_ss_output() -> anyhow::Result<String> {
    let output = Command::new("ss")
        .args(["-tulpn"])
        .output()
        .context("failed to spawn ss -tulpn")?;

    anyhow::ensure!(
        output.status.success(),
        "ss exited with status {}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn scan_ports(output: String) -> Vec<PortProc> {
    let mut results: Vec<PortProc> = Vec::new();

    for line in output.lines() {
        // Skip header
        if line.starts_with("Netid") {
            continue;
        }

        // Skip lines without process info
        if !line.contains("users:((") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();

        let (proc_name, pid) = if let Some(info) = parse_process_info(parts[6]) {
            info
        } else {
            // Skip line if parse failed
            continue;
        };

        let p = PortProc {
            proto: if parts[0] == "tcp" {
                Proto::Tcp
            } else {
                Proto::Udp
            },
            local_address: parts[4].to_string(),
            pid,
            proc_name,
            state: Some(parts[1].to_string()),
        };

        results.push(p);
    }

    results
}

fn parse_process_info(proc_str: &str) -> Option<(String, u32)> {
    let name_start = proc_str.find("((\"")? + 3;
    let name_end = proc_str[name_start..].find("\",")?;
    let proc_name = &proc_str[name_start..name_start + name_end];

    let pid_start = proc_str.find("pid=")? + 4;
    let pid_end = proc_str[pid_start..].find(",")?;
    let pid_str = &proc_str[pid_start..pid_start + pid_end];
    let pid = pid_str.parse::<u32>().ok()?;

    Some((proc_name.to_string(), pid))
}

fn extract_port(local_address: &str) -> Option<u16> {
    local_address
        .rsplit_once(":")
        .and_then(|(_, port_str)| port_str.parse::<u16>().ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SS_OUTPUT: &str = r#"Netid       State        Recv-Q       Send-Q                     Local Address:Port                Peer Address:Port       Process
udp         UNCONN       0            0                            224.0.0.251:5353                     0.0.0.0:*           users:(("chromium",pid=2440,fd=208))
udp         UNCONN       0            0                            224.0.0.251:5353                     0.0.0.0:*           users:(("chromium",pid=2523,fd=57))
udp         UNCONN       0            0                          127.0.0.53%lo:53                       0.0.0.0:*
udp         UNCONN       0            0                    192.168.1.148%wlan0:68                       0.0.0.0:*
tcp         LISTEN       0            4096                           127.0.0.1:45549                    0.0.0.0:*
tcp         LISTEN       0            1024                           127.0.0.1:3000                     0.0.0.0:*           users:(("ruby",pid=27264,fd=6))
tcp         LISTEN       0            1024                               [::1]:3000                        [::]:*           users:(("ruby",pid=27264,fd=7))
"#;

    #[test]
    fn proto_display() {
        assert_eq!(Proto::Tcp.to_string(), "TCP");
        assert_eq!(Proto::Udp.to_string(), "UDP");
    }

    #[test]
    fn proto_display_with_width() {
        assert_eq!(format!("{:<6}", Proto::Tcp), "TCP   ");
        assert_eq!(format!("{:>6}", Proto::Udp), "   UDP");
    }

    #[test]
    fn parse_process_info_success() {
        let proc_str = "users:((\"chromium\",pid=2440,fd=237))";
        let proc_name = String::from("chromium");
        let pid: u32 = 2440;

        assert_eq!(parse_process_info(proc_str), Some((proc_name, pid)));
    }

    #[test]
    fn parse_process_info_from_empty_string() {
        let proc_str = "";
        assert_eq!(parse_process_info(proc_str), None);
    }

    #[test]
    fn parse_process_info_invalid_structure() {
        let proc_str = "users:\"chromium\",pid=2440,fd=237";
        assert_eq!(parse_process_info(proc_str), None);
    }

    #[test]
    fn parse_process_info_invalid_pid() {
        let proc_str = "users:((\"chromium\",pid=abc,fd=237))";
        assert_eq!(parse_process_info(proc_str), None);
    }

    #[test]
    fn scan_ports_with_multiple_entries() {
        let results = scan_ports(SAMPLE_SS_OUTPUT.to_string());
        assert_eq!(results.len(), 4);

        assert_eq!(results[0].proto, Proto::Udp);
        assert_eq!(results[0].local_address, "224.0.0.251:5353");
        assert_eq!(results[0].pid, 2440);
        assert_eq!(results[0].proc_name, "chromium");
        assert_eq!(results[0].state, Some("UNCONN".to_string()));

        assert_eq!(results[1].proto, Proto::Udp);
        assert_eq!(results[1].local_address, "224.0.0.251:5353");
        assert_eq!(results[1].pid, 2523);
        assert_eq!(results[1].proc_name, "chromium");
        assert_eq!(results[1].state, Some("UNCONN".to_string()));

        assert_eq!(results[2].proto, Proto::Tcp);
        assert_eq!(results[2].local_address, "127.0.0.1:3000");
        assert_eq!(results[2].pid, 27264);
        assert_eq!(results[2].proc_name, "ruby");
        assert_eq!(results[2].state, Some("LISTEN".to_string()));

        assert_eq!(results[3].proto, Proto::Tcp);
        assert_eq!(results[3].local_address, "[::1]:3000");
        assert_eq!(results[3].pid, 27264);
        assert_eq!(results[3].proc_name, "ruby");
        assert_eq!(results[3].state, Some("LISTEN".to_string()));
    }

    #[test]
    fn extract_port_from_local_address() {
        assert_eq!(extract_port(""), None);
        assert_eq!(extract_port("224.0.0.251:5353"), Some(5353));
        assert_eq!(extract_port("[::1]:3000"), Some(3000));
        assert_eq!(extract_port("224.0.0.251:abcd"), None);
        assert_eq!(extract_port("invalid-address"), None);
    }
}
