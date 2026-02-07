# 031 — Config Hot-Reload

## Description

After the config write API updates `buddy.toml`, the running server must apply the changes without a restart. This means rebuilding the provider chain, re-registering skills, and refreshing warnings to reflect the new configuration.

## Goal

Users can change models or skills in the Settings UI and see the effects immediately — no server restart required.

## Requirements

- Implement a `reload_from_config()` method (or equivalent) that takes the new `Config` and updates live server state:
  - **Chat providers:** Rebuild the `ProviderChain` from the new `models.chat.providers` list. Replace the active provider chain in `AppState`.
  - **Embedding provider:** If `models.embedding` changed, rebuild the embedder. If embedding was added, initialize the vector store. If removed, disable memory features.
  - **Skills:** Re-register skills based on the new `[skills]` section. Update sandbox rules (allowed_directories, allowed_domains) and approval policies. Add or remove skills as their config appears or disappears.
  - **Chat config:** Update the system prompt used in new conversations.
  - **Memory config:** Update auto-retrieve settings (enabled, limit, threshold).
  - **Warnings:** Clear stale warnings and re-run startup validation checks to emit new warnings reflecting the updated config (e.g., clear `no_embedding_model` if an embedding provider was just added).
- The reload must not interrupt in-flight chat requests. Use `Arc<RwLock<>>` or `ArcSwap` to allow readers to finish with the old state while the new state is being built.
- Call `reload_from_config()` at the end of every successful config write (task 030).
- Server config changes (`host`, `port`) are persisted but require a restart to take effect — display a note in the API response when server config changes.

## Acceptance Criteria

- [ ] Adding a new chat provider via the config API makes it available for the next chat request
- [ ] Removing all but one chat provider updates the `single_chat_provider` warning
- [ ] Adding an embedding provider clears the `no_embedding_model` warning
- [ ] Removing the embedding provider triggers the `no_embedding_model` warning
- [ ] Changing skill sandbox rules (allowed_directories) takes effect for the next skill execution
- [ ] Changing the system prompt takes effect for the next conversation
- [ ] Changing memory config (auto_retrieve, threshold) takes effect immediately
- [ ] In-flight chat requests complete with the old config (no crash or corruption)
- [ ] Server config changes return a note indicating restart is required

## Test Cases

- [ ] Write a config with two chat providers via PUT; send a chat request; assert the provider chain has two entries
- [ ] Write a config adding `[models.embedding]`; call `GET /api/warnings`; assert `no_embedding_model` warning is gone
- [ ] Write a config removing `[models.embedding]`; call `GET /api/warnings`; assert `no_embedding_model` warning appears
- [ ] Write a config changing `skills.read_file.allowed_directories`; execute `read_file` with a path in the new directory; assert success
- [ ] Write a config changing `chat.system_prompt`; start a new chat; assert the new prompt is used
- [ ] Write a config changing `server.port`; assert the response includes a restart-required note
- [ ] Write a config changing `memory.auto_retrieve` to false; send a chat message; assert no `MemoryContext` event is emitted
