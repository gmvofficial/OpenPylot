# OpenPylot bundled sub-agent manifests.
#
# Drop new .toml files into this directory OR into ~/.pylot/agents/ (user) OR
# ./agents/ (workspace). Each manifest defines a preset sub-agent that can be
# spawned from the CLI, the chat, or programmatically via the SDKs.
#
# Precedence (highest wins): workspace > local (~/.pylot/agents) > bundled.
#
# ---
# name              Unique id of the agent. Required.
# description       Short human-readable summary.
# agent_type        task | background | specialist (default: task)
# system_prompt     The system prompt that scopes the agent's behavior.
# model_override    Optional LLM model id (e.g. "gpt-4o", "claude-3-5-sonnet-20241022")
# allowed_tools     Optional list of tool names; null/omitted = inherit all.
# timeout_secs      Hard timeout for this agent (default: 300).
# max_iterations    Max tool-calling loop iterations (default: 10).
# ---
#
# See: coder.toml, researcher.toml, writer.toml, marketer.toml
