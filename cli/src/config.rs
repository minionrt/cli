use core::fmt;
use std::path::PathBuf;
use std::{collections::HashMap, fs};

use anyhow::anyhow;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use url::Url;

static OPENROUTER_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://openrouter.ai/api/v1/chat/completions")
        .expect("Failed to parse OpenRouter chat completions URL")
});
static OPENROUTER_RESPONSES_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://openrouter.ai/api/v1/responses")
        .expect("Failed to parse OpenRouter responses URL")
});

static GEMINI_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions")
        .expect("Failed to parse Gemini chat completions URL")
});
static GEMINI_RESPONSES_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://generativelanguage.googleapis.com/v1beta/openai/responses")
        .expect("Failed to parse Gemini responses URL")
});
static OPENAI_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://api.openai.com/v1/chat/completions")
        .expect("Failed to parse OpenAI chat completions URL")
});
static OPENAI_RESPONSES_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://api.openai.com/v1/responses").expect("Failed to parse OpenAI responses URL")
});

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub llm_provider: Option<LLMProvider>,
    pub openrouter_key: Option<String>,
    pub google_gemini_key: Option<String>,
    pub chatgpt_id_token: Option<String>,
    pub chatgpt_access_token: Option<String>,
    pub chatgpt_refresh_token: Option<String>,
    pub chatgpt_last_refresh_unix_secs: Option<i64>,
    pub chatgpt_api_key: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Debug, Deserialize, Serialize)]
pub enum LLMProvider {
    #[serde(rename = "chatgpt")]
    #[clap(name = "chatgpt")]
    ChatGpt,
    #[serde(rename = "openrouter")]
    #[clap(name = "openrouter")]
    OpenRouter,
    #[serde(rename = "google-gemini")]
    GoogleGemini,
}

impl LLMProvider {
    pub fn tag(&self) -> &'static str {
        match self {
            LLMProvider::ChatGpt => "chatgpt",
            LLMProvider::OpenRouter => "openrouter",
            LLMProvider::GoogleGemini => "google-gemini",
        }
    }
}

impl fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LLMProvider::ChatGpt => write!(f, "ChatGPT"),
            LLMProvider::OpenRouter => write!(f, "OpenRouter"),
            LLMProvider::GoogleGemini => write!(f, "Google Gemini"),
        }
    }
}

pub struct LLMRouterTable {
    pub default_provider: String,
    pub providers: HashMap<String, LLMProviderDetails>,
}

impl LLMRouterTable {
    pub fn details_for_model(&self, provider_and_model: &str) -> (String, &LLMProviderDetails) {
        provider_and_model
            .split_once('/')
            .and_then(|(provider_name, model_name)| {
                self.providers
                    .get(provider_name)
                    .map(|details| (model_name.to_owned(), details))
            })
            .unwrap_or_else(|| {
                (
                    provider_and_model.to_owned(),
                    self.providers
                        .get(&self.default_provider)
                        .expect("Default provider not found"),
                )
            })
    }
}

pub struct LLMProviderDetails {
    pub api_chat_completions_endpoint: Url,
    pub api_responses_endpoint: Url,
    pub api_key: String,
}

impl Config {
    pub fn load_or_create() -> anyhow::Result<Self> {
        match Self::load() {
            Ok(config) => Ok(config),
            Err(_) => {
                let config = Self::default();
                config.save()?;
                Ok(config)
            }
        }
    }

    pub fn load() -> anyhow::Result<Self> {
        let text = fs::read_to_string(Self::filepath()?)?;
        let config = toml::from_str(&text)?;
        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let text = toml::to_string(self)?;
        fs::create_dir_all(Self::filepath()?.parent().unwrap())?;
        fs::write(Self::filepath()?, text)?;
        Ok(())
    }

    pub fn filepath() -> anyhow::Result<PathBuf> {
        Ok(dirs::config_dir()
            .ok_or(anyhow!("Failed to locate appropriate config directory"))?
            .join("minion")
            .join("config.toml"))
    }

    pub fn llm_router_table(&self) -> Option<LLMRouterTable> {
        let mut providers = HashMap::new();

        if let Some(api_key) = self
            .chatgpt_api_key
            .as_ref()
            .or(self.chatgpt_access_token.as_ref())
        {
            providers.insert(
                "chatgpt".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: OPENAI_CHAT_COMPLETIONS_URL.clone(),
                    api_responses_endpoint: OPENAI_RESPONSES_URL.clone(),
                    api_key: api_key.clone(),
                },
            );
        }
        if let Some(key) = &self.openrouter_key {
            providers.insert(
                "openrouter".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: OPENROUTER_CHAT_COMPLETIONS_URL.clone(),
                    api_responses_endpoint: OPENROUTER_RESPONSES_URL.clone(),
                    api_key: key.clone(),
                },
            );
        }
        if let Some(key) = &self.google_gemini_key {
            providers.insert(
                "google-gemini".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: GEMINI_CHAT_COMPLETIONS_URL.clone(),
                    api_responses_endpoint: GEMINI_RESPONSES_URL.clone(),
                    api_key: key.clone(),
                },
            );
        }

        let Some(default_llm_provider) = &self.llm_provider else {
            return None;
        };
        let default_provider = default_llm_provider.tag().to_string();
        if !providers.contains_key(&default_provider) {
            return None;
        }

        Some(LLMRouterTable {
            default_provider,
            providers,
        })
    }
}
