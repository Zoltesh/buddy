# 034 — Settings Page Layout

## Description

Build the Settings page shell with clearly separated sections for Models, Skills, and General settings. The page fetches the current config from `GET /api/config` and provides the structural layout that subsequent tasks (035–037) populate with interactive controls.

## Goal

The Settings page displays the current configuration organized into logical sections, providing the foundation for model and skill management UIs.

## Requirements

- Create a `lib/Settings.svelte` component that replaces the placeholder from task 033
- On mount, fetch the current config from `GET /api/config`
- Display a loading state while fetching and an error state if the fetch fails
- Organize settings into tabbed or accordion sections:
  - **Models** — Chat providers and embedding providers (task 035 adds full interactivity)
  - **Skills** — Skill configurations (task 037 adds full interactivity)
  - **General** — Server settings, chat config (system prompt), memory settings
- Each section displays the current config values in a read-friendly format:
  - Models: list provider type, model name, endpoint for each slot
  - Skills: show which skills are configured and their sandbox rules
  - General: show host, port, system prompt, memory settings
- The General section should be fully interactive in this task:
  - Editable fields for system prompt, memory auto-retrieve toggle, similarity threshold, auto-retrieve limit
  - Save button that PUTs to the appropriate config endpoints
  - Success/error feedback after save
- Style with Tailwind CSS, consistent with the existing chat UI aesthetic
- Responsive layout that works on both desktop and mobile widths

## Acceptance Criteria

- [ ] Settings page loads and displays current config from `GET /api/config`
- [ ] Loading and error states are handled gracefully
- [ ] Models section displays current chat and embedding providers
- [ ] Skills section displays current skill configurations
- [ ] General section shows server, chat, and memory settings
- [ ] General section fields are editable and save via the config write API
- [ ] Save success shows a confirmation message
- [ ] Save failure shows the validation error(s) from the API
- [ ] Layout is responsive and visually consistent with the chat UI

## Test Cases

- [ ] Load settings page; assert `GET /api/config` is called and sections are populated
- [ ] Load settings with a config that has no embedding providers; assert the embedding section shows "Not configured"
- [ ] Edit the system prompt in the General section; click Save; assert `PUT /api/config/chat` is called with the new value
- [ ] Edit memory settings; click Save; assert `PUT /api/config/memory` is called
- [ ] Simulate a failed `GET /api/config`; assert an error message is displayed
- [ ] Simulate a validation error on save; assert the error details are shown inline
