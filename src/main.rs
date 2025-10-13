use clap::Parser;

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
        println!("Running --once");
    } else if let Some(interval) = cli.interval {
        println!("Running --interval with {} seconds", interval);
    } else {
        println!("Running default");
    }

    println!("Portwatch");
}
