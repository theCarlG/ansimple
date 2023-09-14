use clap::Parser;
use tokio::process;

mod playbook;
mod task;

use std::path::PathBuf;

use self::playbook::{HostConfig, Playbook};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'c', long)]
    host_config: Option<PathBuf>,

    #[arg(short = 's', long)]
    host_script: Option<PathBuf>,

    #[arg(short = 't', long, value_delimiter = ',')]
    tags: Option<Vec<String>>,

    playbook: PathBuf,
}

#[tokio::main]
async fn main() {
    let cli = Args::parse();

    let host_config = if let Some(host_script) = cli.host_script {
        let output = process::Command::new(host_script)
            .output()
            .await
            .expect("failed to execute host_script");

        HostConfig::try_from(output.stdout).expect("failed to read host_config")
    } else {
        let host_config = cli.host_config.expect("no host_config specified");
        HostConfig::try_from(host_config).expect("failed to read host_config")
    };

    let mut config = Playbook::try_from(cli.playbook).expect("failed to read config");
    config.process(host_config, cli.tags).await;
}
