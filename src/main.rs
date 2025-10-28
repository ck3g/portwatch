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
        Ok(stdout) => println!("{stdout}"),
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
