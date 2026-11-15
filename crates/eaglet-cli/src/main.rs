//! `eaglet`: create eaglet projects and generate their parts.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use eaglet_cli::plan::Plan;
use eaglet_cli::{Db, Kind, Source, plan_generate, plan_new};

/// Create eaglet projects and generate modules, controllers, services and resources.
#[derive(Parser)]
#[command(name = "eaglet", version = eaglet::VERSION, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a project: `eaglet new shop --db postgres`.
    New {
        /// The project folder and crate name.
        name: String,
        /// The database to set up.
        #[arg(long, value_enum, default_value_t = DbArg::None)]
        db: DbArg,
        /// Use a local eaglet checkout instead of crates.io (path to its `crates/eaglet`).
        #[arg(long, value_name = "DIR")]
        eaglet_path: Option<PathBuf>,
        /// Print the files without writing them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate part of a project: `eaglet g resource products`.
    #[command(alias = "g")]
    Generate {
        /// What to generate.
        #[arg(value_enum)]
        kind: KindArg,
        /// Its name, for example `products` or `order-items`.
        name: String,
        /// The project folder.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Print the files without writing them.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum DbArg {
    None,
    Postgres,
    Sqlite,
    Mongo,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    #[value(alias = "mo")]
    Module,
    #[value(alias = "co")]
    Controller,
    #[value(alias = "s")]
    Service,
    #[value(alias = "res")]
    Resource,
}

fn finish(plan: &Plan, dry_run: bool) -> Result<(), String> {
    for line in plan.summary() {
        println!("{line}");
    }
    if dry_run {
        println!("(dry run: nothing was written)");
        return Ok(());
    }
    for note in plan.write()? {
        eprintln!("note: {note}");
    }
    Ok(())
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::New {
            name,
            db,
            eaglet_path,
            dry_run,
        } => {
            let source = match eaglet_path {
                Some(p) => {
                    let p = p
                        .canonicalize()
                        .map_err(|e| format!("--eaglet-path {}: {e}", p.display()))?;
                    Source::Path(p.display().to_string())
                }
                None => Source::Registry,
            };
            let db = match db {
                DbArg::None => Db::None,
                DbArg::Postgres => Db::Postgres,
                DbArg::Sqlite => Db::Sqlite,
                DbArg::Mongo => Db::Mongo,
            };
            let plan = plan_new(
                &std::env::current_dir().map_err(|e| e.to_string())?,
                &name,
                db,
                &source,
            )?;
            finish(&plan, dry_run)?;
            if !dry_run {
                println!(
                    "\nNext: cd {name} && cargo run   (API docs at http://localhost:3000/docs)"
                );
            }
            Ok(())
        }
        Command::Generate {
            kind,
            name,
            path,
            dry_run,
        } => {
            let kind = match kind {
                KindArg::Module => Kind::Module,
                KindArg::Controller => Kind::Controller,
                KindArg::Service => Kind::Service,
                KindArg::Resource => Kind::Resource,
            };
            finish(&plan_generate(&path, kind, &name)?, dry_run)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
