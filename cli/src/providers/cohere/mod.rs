use std::io::{self, Write};

use crate::config::Config;

const COHERE_API_KEYS_URL: &str = "https://dashboard.cohere.ai/api-keys";

pub async fn login_flow(mut config: Config) -> anyhow::Result<()> {
    println!("Opening Cohere API Keys page in your default web browser.");
    println!("If it doesn't open automatically, please visit: {COHERE_API_KEYS_URL}");
    if let Err(err) = webbrowser::open(COHERE_API_KEYS_URL) {
        eprintln!("Failed to open browser: {err}");
    }

    print!("Please enter your Cohere API key: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();

    if input.is_empty() {
        return Err(anyhow::anyhow!("No API key provided."));
    }

    config.cohere_key = Some(input);
    if config.llm_provider.is_none() {
        println!("Cohere is now your default LLM provider.");
        config.llm_provider = Some(crate::config::LLMProvider::Cohere);
    }
    config.save()?;

    println!("Your Cohere API key has been saved to the config file at:");
    println!(
        "{}",
        Config::filepath()
            .expect("Failed to get config file path")
            .to_string_lossy()
    );
    Ok(())
}
