use std::path::PathBuf;
use std::process;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
struct App {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ci(CiOptions),
}

#[derive(Debug, Args)]
struct CiOptions {
    #[clap(long, short, default_value = "checks")]
    workflow: String,

    #[clap(long, short)]
    job: String,
}

fn main() {
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
        }
    }
}
