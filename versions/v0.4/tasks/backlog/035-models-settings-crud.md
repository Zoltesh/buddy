# 035 — Models Settings: Display and CRUD

## Description

Build the interactive Models section of the Settings page. Users can view the ordered list of providers for each model slot (chat, embedding), add new providers, edit existing ones, and delete providers. Each provider can be tested for connectivity before saving.

## Goal

Users can fully manage their model provider configurations from the browser — adding, editing, and removing providers for both chat and embedding slots.

## Requirements

- In the Models section of the Settings page, display each model slot (Chat, Embedding) as a distinct sub-section
- Each sub-section shows its providers as an ordered list (cards or rows) with:
  - Provider type (openai, lmstudio, local)
  - Model name
  - Endpoint (if set)
  - API key env var name (if set; never the actual key)
  - Position indicator (primary / fallback #2, #3, etc.)
- **Add provider:** A button at the bottom of each slot's list opens an inline form or modal with fields:
  - Provider type (dropdown: openai, lmstudio, local)
  - Model name (text input, required)
  - Endpoint (text input, optional — show sensible placeholder per provider type)
  - API key env var (text input, optional — only shown for types that need it)
- **Edit provider:** Clicking an existing provider opens the same form pre-filled with current values
- **Delete provider:** Each provider has a delete button with a confirmation step (e.g., "Are you sure?")
- **Connection test:** The add/edit form includes a "Test Connection" button that calls `POST /api/config/test-provider` and displays the result inline (green check / red X with message)
- **Save:** After any add/edit/delete, the full updated provider list for that slot is sent to `PUT /api/config/models`
- Validation:
  - Chat slot must always have at least one provider — prevent deleting the last one
  - Model name is required and must not be empty
  - Show validation errors inline next to the relevant field
- The embedding slot can be empty (shows "Not configured" with an "Add Provider" button)

## Acceptance Criteria

- [ ] Chat providers are displayed as an ordered list with type, model, endpoint, and key env var
- [ ] Embedding providers are displayed similarly, or "Not configured" if empty
- [ ] Users can add a new provider to a slot via an inline form
- [ ] Users can edit an existing provider's configuration
- [ ] Users can delete a provider (with confirmation)
- [ ] Deleting the last chat provider is prevented with an error message
- [ ] "Test Connection" button calls the test endpoint and shows pass/fail inline
- [ ] Saving calls `PUT /api/config/models` and shows success/error feedback
- [ ] Form fields validate inline (required model name, etc.)
- [ ] Provider position (primary vs fallback) is clearly indicated

## Test Cases

- [ ] Load settings with two chat providers; assert both are displayed in order with correct details
- [ ] Click "Add Provider" on the chat slot; fill in the form; click Save; assert `PUT /api/config/models` is called with three providers
- [ ] Click edit on a provider; change the model name; save; assert the updated config is sent
- [ ] Click delete on a fallback provider; confirm; assert the provider is removed and config is saved
- [ ] Attempt to delete the only chat provider; assert an error prevents it
- [ ] Click "Test Connection" with a valid provider; assert success feedback is displayed
- [ ] Click "Test Connection" with an unreachable endpoint; assert failure feedback is displayed
- [ ] Submit the add form with an empty model name; assert inline validation error
- [ ] Load settings with no embedding providers; assert "Not configured" is shown with an Add button
