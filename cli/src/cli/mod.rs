use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::config::{Config, LLMProvider};
use crate::providers::{chatgpt, gemini, openrouter};

mod editor;
mod run;

#[derive(Args, Debug, Clone, Default)]
struct RunArgs {
    /// Task description
    #[arg(short = 'm', long, global = true, help_heading = "Run Options")]
    message: Option<String>,

    /// Use the Containerfile located at the specified path
    #[arg(long, global = true, help_heading = "Run Options")]
    containerfile: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a task in the current directory
    #[command(name = "run", alias = "")]
    Run,

    /// Login using one of the supported LLM providers
    Login {
        #[arg(value_enum)]
        llm_provider: LLMProvider,
    },
}

#[derive(Parser)]
#[command(version, author, about, long_about = None)]
struct Cli {
    /// Enable trace logging
    #[arg(long)]
    trace: bool,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    #[command(flatten)]
    run: RunArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    fn invalid_use_of_run_args(&self) -> bool {
        let is_run_command = matches!(self.command, Some(Command::Run)) || self.command.is_none();

        !is_run_command && (self.run.message.is_some() || self.run.containerfile.is_some())
    }
}

pub fn exec() {
    let cli = Cli::parse();
    let mut builder = env_logger::Builder::from_default_env();
    builder
        .format_timestamp(None)
        .format_level(false)
        .format_target(false);

    if cli.trace {
        builder.filter_level(log::LevelFilter::Trace);
    } else if cli.debug {
        builder.filter_level(log::LevelFilter::Debug);
    } else {
        builder.filter_level(log::LevelFilter::Warn);
    }

    builder.init();

    if cli.invalid_use_of_run_args() {
        eprintln!("Run options are only valid with `minion` or `minion run`.");
        std::process::exit(2);
    }

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            let mut config = Config::load_or_create().expect("Failed to load config");

            tokio::runtime::Runtime::new()
                .expect("Failed to create runtime")
                .block_on(async {
                    chatgpt::refresh_if_needed(&mut config)
                        .await
                        .expect("Failed to refresh ChatGPT login");
                });

            let Some(llm_router_table) = config.llm_router_table() else {
                eprintln!("You currently don't have a LLM API key configured.");
                eprintln!("Run `minion login` to authenticate with a supported provider.");
                eprintln!(
                    "Supported providers: {}",
                    LLMProvider::value_variants()
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                eprintln!("If your LLM provider is not listed, please contribute!");
                std::process::exit(1);
            };

            let task_description = if let Some(msg) = cli.run.message {
                msg
            } else {
                read_task_from_editor()
            };

            println!("{task_description}");
            println!();

            println!("Working on the task.");

            tokio::runtime::Runtime::new()
                .expect("Failed to create runtime")
                .block_on(async {
                    run::run(
                        llm_router_table,
                        &cli.run.containerfile,
                        &std::env::current_dir().expect("Failed to get current dir"),
                        task_description,
                    )
                    .await
                    .expect("Failed to run task");
                });
        }
        Command::Login {
            llm_provider: provider,
        } => {
            tokio::runtime::Runtime::new()
                .expect("Failed to create runtime")
                .block_on(async {
                    let config = Config::load_or_create().expect("Failed to load config");
                    match provider {
                        LLMProvider::ChatGpt => chatgpt::login_flow(config)
                            .await
                            .expect("Failed to start login flow"),
                        LLMProvider::OpenRouter => openrouter::login_flow(config)
                            .await
                            .expect("Failed to start login flow"),
                        LLMProvider::GoogleGemini => gemini::login_flow(config)
                            .await
                            .expect("Failed to start login flow"),
                    }
                });
        }
    }
}

fn read_task_from_editor() -> String {
    let initial_message =
        "\n\n# Please describe your task. Lines starting with '#' will be ignored.";
    let edited = editor::Editor::new()
        .edit(initial_message)
        .unwrap_or_else(|err| {
            eprintln!("Failed to open editor: {err}");
            std::process::exit(1);
        });

    let edited = edited
        .map(|text| {
            text.lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .expect("Failed to read from editor");

    let trimmed = edited.trim();

    if trimmed.is_empty() {
        eprintln!("No input received.");
        std::process::exit(1);
    }

    trimmed.to_owned()
}
