use anyhow::Context;
use clap::Parser;
use std::process::Command;

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

#[derive(Debug, Clone)]
enum Proto {
    Tcp,
    Udp,
}

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
        println!("Running --interval with {} seconds", interval);
    } else {
        println!("Running default");
    }
}

fn handle_once() {
    match fetch_ss_output() {
        Ok(stdout) => {
            let pp = scan_ports(stdout);
            for p in pp {
                println!("{:?}", p);
            }
        }
        Err(err) => eprintln!("scan failed: {err}"),
    }
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
    println!("{}", output);
    let pp = vec![
        PortProc {
            proto: Proto::Udp,
            local_address: String::from("224.0.0.251:5353"),
            pid: 123,
            proc_name: String::from("chromium"),
            state: Some("UNCONN".to_string()),
        },
        PortProc {
            proto: Proto::Tcp,
            local_address: String::from("127.0.0.1:6000"),
            pid: 123,
            proc_name: String::from("FakeProc"),
            state: Some("LISTEN".to_string()),
        },
    ];

    pp
}
