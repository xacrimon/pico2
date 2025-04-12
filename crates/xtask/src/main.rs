use std::path::PathBuf;
use std::process::{self, ExitCode};

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
struct App {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ci(CiOptions),
    Picoremote(PicoremoteOptions),
}

#[derive(Debug, Args)]
struct CiOptions {
    #[clap(long, short, default_value = "checks")]
    workflow: String,

    #[clap(long, short)]
    job: String,
}

#[derive(Debug, Args)]
struct PicoremoteOptions {
    #[clap(subcommand)]
    command: PicoremoteCommand,
}

#[derive(Debug, Subcommand)]
enum PicoremoteCommand {
    Build,
    Start,
    Stop,
}

fn main() -> ExitCode {
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
        Command::Picoremote(options) => match options.command {
            PicoremoteCommand::Build => {
                process::Command::new("docker")
                    .arg("build")
                    .arg("--tag")
                    .arg("picoremote")
                    .arg("./picoremote")
                    .status()
                    .expect("docker build failed");
            }
            PicoremoteCommand::Start => {
                let inspect_status = process::Command::new("docker")
                    .arg("inspect")
                    .arg("xtask-picoremote-server")
                    .arg("--format")
                    .arg("{{.State.Running}}")
                    .status()
                    .expect("docker inspect failed");

                if inspect_status.success() {
                    println!(
                        "container with name xtask-picoremote-server already exists. delete it and try again"
                    );

                    return ExitCode::FAILURE;
                }

                process::Command::new("docker")
                    .arg("run")
                    .arg("--name")
                    .arg("xtask-picoremote-server")
                    .arg("--detach")
                    .arg("picoremote")
                    .status()
                    .expect("docker run failed");
            }
            PicoremoteCommand::Stop => {
                process::Command::new("docker")
                    .arg("stop")
                    .arg("xtask-picoremote-server")
                    .status()
                    .expect("docker stop failed");

                process::Command::new("docker")
                    .arg("container")
                    .arg("rm")
                    .arg("xtask-picoremote-server")
                    .status()
                    .expect("docker rm failed");
            }
        },
    }

    ExitCode::SUCCESS
}
