#!/bin/sh
set -eu

export CODEX_HOME="/tmp/codex-home"
mkdir -p "$CODEX_HOME"

export OPENAI_API_KEY="$MINION_API_TOKEN"
export OPENAI_BASE_URL="$MINION_API_BASE_URL"

jq -n \
  --arg api_key "$OPENAI_API_KEY" \
  '{
    OPENAI_API_KEY: $api_key,
    tokens: null,
    last_refresh: null
  }' > "$CODEX_HOME/auth.json"

exec /usr/local/bin/acp2rt \
  --workspace-path /workspace \
  -- \
  /usr/local/bin/codex-acp \
  -c 'model_provider="openai"' \
  -c 'approval_policy="never"' \
  # We are running codex in a containerized environment where we can grant the agent full virtual disk and network access.
  # If desired, network access should be restricted in the container runtime.
  # Codex' own "sandbox" cannot reliably guarantee these restrictions anyway (at the time of this writing).
  -c 'sandbox_mode="danger-full-access"'
