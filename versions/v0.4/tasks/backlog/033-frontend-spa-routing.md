# 033 — Frontend SPA Routing

## Description

Add client-side routing to the Svelte frontend so the application can have multiple pages. Currently the app is a single-page chat view. V0.4 needs at least two routes: `/` for chat and `/settings` for configuration.

## Goal

The frontend supports multiple routes with client-side navigation. Users can navigate between chat and settings without full page reloads.

## Requirements

- Add a client-side router to the Svelte app (use `svelte-spa-router` or a lightweight equivalent)
- Define routes:
  - `/` — Chat page (existing `App.svelte` content, extracted into a `Chat.svelte` component)
  - `/settings` — Settings page (placeholder for now; task 034 builds it out)
- Extract the chat UI from `App.svelte` into a `lib/Chat.svelte` component
- `App.svelte` becomes the router shell: renders the sidebar (always visible) and the routed page content
- Add a "Settings" link/icon to the sidebar (`lib/Sidebar.svelte`) that navigates to `/settings`
- Add a "Back to Chat" link on the settings page (or make the sidebar chat links navigate back)
- Ensure the Axum backend serves `index.html` for all non-API routes (SPA fallback) — verify the existing `ServeDir` fallback handles this correctly
- Hash-based routing (`/#/settings`) is acceptable and simpler for static file serving; choose based on what works cleanly with the existing Axum static file setup

## Acceptance Criteria

- [ ] Navigating to `/` shows the chat interface
- [ ] Navigating to `/settings` shows a settings placeholder page
- [ ] The sidebar is visible on both pages
- [ ] A "Settings" link in the sidebar navigates to `/settings`
- [ ] Navigation between routes does not trigger a full page reload
- [ ] Browser back/forward buttons work correctly
- [ ] The Axum backend correctly serves the SPA for non-API routes
- [ ] Existing chat functionality is unaffected

## Test Cases

- [ ] Load the app at `/`; assert the chat interface renders (conversation list, message input)
- [ ] Click the Settings link in the sidebar; assert the URL changes and the settings placeholder appears
- [ ] Navigate to `/settings` directly via URL; assert the settings page renders
- [ ] From settings, navigate back to chat; assert the chat interface renders with state preserved
- [ ] Send a `GET` request to `/settings` on the backend; assert it returns `index.html` (SPA fallback)
