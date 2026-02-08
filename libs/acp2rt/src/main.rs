use std::ffi::OsString;
use std::sync::Arc;
use std::{env, path::PathBuf};

use clap::Parser;
use tokio::process::Command as TokioCommand;

use acp2rt::{Agent, AgentConfig};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long)]
    workspace_path: PathBuf,
    #[arg(required = true, trailing_var_arg = true)]
    command: Vec<OsString>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let command = Arc::new(args.command);

    let api_base_url = env::var("MINION_API_BASE_URL")?;
    let api_base_url_parsed = url::Url::parse(&api_base_url)?;
    let api_token = env::var("MINION_API_TOKEN")?;
    let subcommand_api_base_url = api_base_url.clone();
    let subcommand_api_token = api_token.clone();

    let config = AgentConfig::new(
        {
            let command = Arc::clone(&command);
            move || {
                let mut cmd = TokioCommand::new(&command[0]);
                if command.len() > 1 {
                    cmd.args(&command[1..]);
                }
                cmd.env("OPENAI_API_KEY", &subcommand_api_token);
                cmd.env("OPENAI_BASE_URL", &subcommand_api_base_url);
                cmd
            }
        },
        api_base_url_parsed,
        api_token,
        args.workspace_path,
    );

    let agent = Agent::new(config);
    let _ = agent.run_once().await?;
    Ok(())
}
