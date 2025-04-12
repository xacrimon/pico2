mod picoremote;

use std::path::PathBuf;
use std::process::{self};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
struct App {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ci(CiOptions),
    Picoremote(picoremote::Options),
}

#[derive(Debug, Args)]
struct CiOptions {
    #[clap(long, short, default_value = "checks")]
    workflow: String,

    #[clap(long, short)]
    job: String,
}

fn main() -> Result<()> {
    let app = App::parse();

    match app.command {
        Command::Ci(options) => {
            let workflow_file =
                PathBuf::from(format!(".github/workflows/{}.yml", options.workflow));

            process::Command::new("act")
                .arg("--workflows")
                .arg(workflow_file)
                .arg("--job")
                .arg(options.job)
                .status()
                .expect("act failed");

            Ok(())
        }
        Command::Picoremote(options) => picoremote::handle(&options),
    }
}
