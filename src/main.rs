use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Run once and print port table, then exit")]
    once: bool,
}

fn main() {
    let cli = Args::parse();
    if cli.once {
        println!("Running --once");
    } else {
        println!("Running default");
    }

    println!("Portwatch");
}
