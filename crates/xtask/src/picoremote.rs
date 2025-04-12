use std::process::{self, ExitCode};

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct Options {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Build,
    Start,
    Stop,
}

pub fn handle(options: &Options) -> ExitCode {
    match options.command {
        Command::Build => {
            process::Command::new("docker")
                .arg("build")
                .arg("--tag")
                .arg("picoremote")
                .arg("./picoremote")
                .status()
                .expect("docker build failed");
        }
        Command::Start => {
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
        Command::Stop => {
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
    }

    ExitCode::SUCCESS
}
