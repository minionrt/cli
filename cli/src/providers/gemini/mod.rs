use std::io::{self, Write};

use crate::config::Config;

const AISTUDIO_API_KEYS_URL: &str = "https://aistudio.google.com/app/apikey";

pub async fn login_flow(mut config: Config) -> anyhow::Result<()> {
    println!("Google AI Studio should open in your default web browser.");
    println!("If it doesn't, please visit: {AISTUDIO_API_KEYS_URL}");
    println!("You may need to create a Google Cloud project first.");
    println!("You can do so at: https://console.cloud.google.com");

    if let Err(err) = webbrowser::open(AISTUDIO_API_KEYS_URL) {
        eprintln!("Failed to open browser: {err}");
    }

    print!("Please enter your Gemini API key: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();

    if input.is_empty() {
        return Err(anyhow::anyhow!("No API key provided."));
    }

    // Store the key in the config and save it
    config.google_gemini_key = Some(input);
    if config.llm_provider.is_none() {
        println!("Google Gemini is now your default LLM provider.");
        config.llm_provider = Some(crate::config::LLMProvider::GoogleGemini);
    }
    config.save()?;

    println!("Your Gemini API key has been saved to the config file at:");
    println!(
        "{}",
        Config::filepath()
            .expect("Failed to get config file path")
            .to_string_lossy()
    );
    Ok(())
}
