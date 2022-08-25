pub mod libgen_cli;
use clap::Parser;

#[tokio::main]
async fn main() {
    match libgen_cli::init(&libgen_cli::cli_args::Args::parse()).await {
        Ok(_) => (),
        Err(err) => println!("{}", err),
    }
}
