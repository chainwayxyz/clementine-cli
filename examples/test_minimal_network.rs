use clap::Parser;
use clementine_cli::cli_network_minimal::CliNetwork;

#[derive(Parser)]
#[command(name = "test")]
struct Args {
    #[arg(long, value_enum)]
    network: Option<CliNetwork>,

    label: String,
}

fn main() {
    let args = Args::parse();
    println!("Network: {:?}", args.network);
    println!("Label: {}", args.label);
}
