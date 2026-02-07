use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OpenAI-compatible completion request.
///
/// References:
/// * https://platform.openai.com/docs/api-reference/chat
/// * https://openrouter.ai/docs/requests
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompletionRequest {
    /// Either "messages" or "prompt" is required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,

    /// Either "messages" or "prompt" is required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// If "model" is unspecified, uses the user's default
    /// See "Supported Models" section
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Allows to force the model to produce a specific output format.
    /// See models page and note on this docs page for which models support it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,

    /// Stop: string or string[]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Stop>,

    /// Enable streaming
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    // ------------------------------------------------------------------
    // See LLM Parameters (openrouter.ai/docs/parameters)
    // ------------------------------------------------------------------
    /// Range: [1, context_length)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Range: [0, 2]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    // ------------------------------------------------------------------
    // Tool calling
    // Will be passed down as-is for providers implementing OpenAI's interface.
    // For providers with custom interfaces, we transform and map the properties.
    // Otherwise, we transform the tools into a YAML template. The model responds with an assistant message.
    // See models supporting tool calling: openrouter.ai/models?supported_parameters=tools
    // ------------------------------------------------------------------
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    // ------------------------------------------------------------------
    // Advanced optional parameters
    // ------------------------------------------------------------------
    /// Integer only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    /// Range: (0, 1]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Range: [1, Infinity). Not available for OpenAI models
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    /// Range: [-2, 2]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,

    /// Range: [-2, 2]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,

    /// Range: (0, 2]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repetition_penalty: Option<f32>,

    /// Keyed by token ID. Mapped to a bias value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<i64, f32>>,

    /// Integer only
    ///
    /// (Note: The TS spec does not mark it optional,
    /// but we often make it `Option` in Rust for real-world usage.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,

    /// Range: [0, 1]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_p: Option<f32>,

    /// Range: [0, 1]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_a: Option<f32>,

    // ------------------------------------------------------------------
    // Reduce latency by providing the model with a predicted output
    // https://platform.openai.com/docs/guides/latency-optimization#use-predicted-outputs
    // ------------------------------------------------------------------
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction: Option<Prediction>,
}

/// Matches `response_format?: { type: 'json_object' }`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseFormat {
    /// Must be `"json_object"` in the OpenRouter spec
    #[serde(rename = "type")]
    pub response_type: String,
}

/// Matches `stop?: string | string[]`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Stop {
    Single(String),
    Multiple(Vec<String>),
}

/// Matches `prediction?: { type: 'content'; content: string }`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Prediction {
    /// Must be "content" for the predicted output
    #[serde(rename = "type")]
    pub prediction_type: String,
    pub content: String,
}

/// Matches the `tools?: Tool[]` array in the TypeScript schema.
#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    /// Must be "function" in the TypeScript schema
    #[serde(rename = "type")]
    pub tool_type: String,

    /// The function details
    #[serde(rename = "function")]
    pub function_desc: FunctionDescription,
}

/// Matches the `function` object { name, description?, parameters } in a Tool
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDescription {
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Name of the function
    pub name: String,

    /// A JSON Schema object describing function parameters
    pub parameters: serde_json::Value,
}

/// Matches `tool_choice?: ToolChoice`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// 'none'
    None(String),
    /// 'auto'
    Auto(String),
    /// { type: 'function'; function: { name: string } }
    FunctionCall {
        #[serde(rename = "type")]
        choice_type: String,
        function: FunctionName,
    },
}

/// Nested object for the 'function' call: { name: string }
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionName {
    pub name: String,
}

/// Represents a single message, which can be one of:
///   - user | assistant | system
///   - tool
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Message {
    /// "user" | "assistant" | "system" | "tool"
    pub role: String,

    /// Content depends on the role.
    ///  - For user/assistant/system: can be a string or a list of content parts.
    ///  - For tool: should be a plain string.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,

    /// Used when role is "tool". Connects this message to the tool call ID above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    /// If "name" is included, it may be prepended for non-OpenAI models like: "{name}: {content}"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Either a direct string or an array of structured content parts.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Just a plain string for content
    Text(String),
    /// Or a vector of parts (text/image)
    Parts(Vec<ContentPart>),
}

/// The union of text or image URL content:
///   { type: 'text', text: string }
///   { type: 'image_url', image_url: { url, detail? } }
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// type: 'text'
    Text {
        /// The text body
        text: String,
    },
    /// type: 'image_url'
    ImageUrl {
        /// Contains the URL or base64 data
        image_url: ImageUrl,
    },
}

/// Inner object for the image_url content
#[derive(Debug, Serialize, Deserialize)]
pub struct ImageUrl {
    /// URL or base64 encoded image data
    pub url: String,

    /// Optional. Defaults to "auto"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
