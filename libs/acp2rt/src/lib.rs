//! This library connects an agent that supports the Agent Client Protocol (ACP, https://agentclientprotocol.com)
//! with the minionrt runtime API (libs/agent-api).

mod acp_client;
mod agent;
mod config;

pub type AcpResult<T> = agent_client_protocol::Result<T>;

pub use acp_client::ACPClient;
pub use agent::{Agent, RunOutcome};
pub use config::AgentConfig;
