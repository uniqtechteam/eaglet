//! `eaglet`: create eaglet projects and generate their parts.

use clap::Parser;

/// Create eaglet projects and generate modules, controllers, services and resources.
#[derive(Parser)]
#[command(name = "eaglet", version = eaglet::VERSION, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
