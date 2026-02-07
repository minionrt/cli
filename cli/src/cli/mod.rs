use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{Config, LLMProvider};
use crate::providers::{cohere, gemini, groq, openrouter};

mod editor;
mod run;

#[derive(Subcommand)]
enum Command {
    /// Run a task in the current directory
    #[clap(name = "run", alias = "")]
    Run {
        /// Task description
        #[clap(short = 'm')]
        message: Option<String>,
        /// Use the Containerfile located at the specified path
        #[clap(long)]
        containerfile: Option<PathBuf>,
    },
    /// Login using one of the supported LLM providers
    Login {
        #[clap(value_enum)]
        llm_provider: LLMProvider,
    },
}

#[derive(Parser)]
#[clap(version, author, about, long_about = None)]
struct Cli {
    /// Enable trace logging
    #[clap(long)]
    trace: bool,
    /// Enable debug logging
    #[clap(long)]
    debug: bool,
    #[clap(subcommand)]
    command: Option<Command>,
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

    match cli.command.unwrap_or(Command::Run {
        message: None,
        containerfile: None,
    }) {
        Command::Run {
            message,
            containerfile,
        } => {
            let config = Config::load_or_create().expect("Failed to load config");
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

            let task_description = if let Some(msg) = message {
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
                        &containerfile,
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
                        LLMProvider::OpenRouter => openrouter::login_flow(config)
                            .await
                            .expect("Failed to start login flow"),
                        LLMProvider::Groq => groq::login_flow(config)
                            .await
                            .expect("Failed to start login flow"),
                        LLMProvider::GoogleGemini => gemini::login_flow(config)
                            .await
                            .expect("Failed to start login flow"),
                        LLMProvider::Cohere => cohere::login_flow(config)
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
