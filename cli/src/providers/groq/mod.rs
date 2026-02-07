use std::io::{self, Write};

use crate::config::Config;

const GROQ_API_KEYS_URL: &str = "https://console.groq.com/keys";

pub async fn login_flow(mut config: Config) -> anyhow::Result<()> {
    println!("The Groq console should open in your default web browser.");
    println!("If it doesn't, please visit: {GROQ_API_KEYS_URL}");

    if let Err(err) = webbrowser::open(GROQ_API_KEYS_URL) {
        eprintln!("Failed to open browser: {err}");
    }

    print!("Please enter your Groq API key: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();

    if input.is_empty() {
        return Err(anyhow::anyhow!("No API key provided."));
    }

    // Store the key in the config and save it
    config.groq_key = Some(input);
    if config.llm_provider.is_none() {
        println!("Groq is now your default LLM provider.");
        config.llm_provider = Some(crate::config::LLMProvider::Groq);
    }
    config.save()?;

    println!("Your Groq API key has been saved to the config file at:");
    println!(
        "{}",
        Config::filepath()
            .expect("Failed to get config file path")
            .to_string_lossy()
    );
    Ok(())
}
