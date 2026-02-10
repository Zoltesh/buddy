# 063 — Interfaces Route and Navigation

## Description

V0.5 adds Telegram and WhatsApp as communication interfaces. The web UI needs a place to manage them. The V0.4 application shell established a navigation rail with Chat and Settings. This task adds a third nav item — "Interfaces" — with its own route (`#/interfaces`) and page layout. The page structure is established here; the actual interface management cards come in the next task (064).

## Goal

The navigation rail has three items (Chat, Interfaces, Settings) and the Interfaces page renders with a header and placeholder content.

## Requirements

- Add an "Interfaces" nav item to the sidebar navigation rail in `Sidebar.svelte`:
  - Position: between Chat and Settings
  - Icon: a connection/link icon (e.g., a chain link or network icon — use an SVG inline)
  - Label: "Interfaces"
  - Route: `#/interfaces`
  - Active state highlighting (same pattern as Chat and Settings)
  - Tooltip in collapsed mode
- Add the `#/interfaces` route to `App.svelte`:
  - Renders an `Interfaces` component when the route is `/interfaces`
  - Follow the same pattern as the Settings conditional render
- Create `frontend/src/lib/Interfaces.svelte`:
  - Page header: "Interfaces" with a subtitle "Manage connected messaging channels"
  - Empty state: a centered message "No interfaces configured. Add Telegram or WhatsApp in your buddy.toml to get started." with a link to Settings > General
  - The component accepts a `config` prop (or fetches config on mount) to determine which interfaces are configured
  - Layout: similar to Settings page styling (consistent with the app's design language)
- **Sidebar context panel:** When the Interfaces route is active, the sidebar context panel (below the nav rail) is empty — same as when Settings is active
- **No backend changes.** This is a frontend-only task.

## Acceptance Criteria

- [ ] Navigation rail has three items: Chat, Interfaces, Settings
- [ ] "Interfaces" nav item has an icon, label, and links to `#/interfaces`
- [ ] Clicking "Interfaces" navigates to the route and highlights the nav item
- [ ] The `#/interfaces` route renders the Interfaces component
- [ ] Interfaces page has a header and subtitle
- [ ] When no interfaces are configured, an empty state message is shown
- [ ] Sidebar context panel is empty when Interfaces route is active
- [ ] Collapsed sidebar shows the Interfaces icon with tooltip
- [ ] All existing navigation functionality works (Chat, Settings, conversation switching)

## Test Cases

- [ ] Load the app; sidebar shows three nav items: Chat, Interfaces, Settings (in that order)
- [ ] Click "Interfaces" nav item; URL changes to `#/interfaces`, Interfaces page renders, nav item is highlighted
- [ ] Click "Chat" from Interfaces page; URL changes to `#/`, Chat renders, conversation list appears in sidebar
- [ ] Click "Settings" from Interfaces page; URL changes to `#/settings`, Settings renders
- [ ] Collapse the sidebar; Interfaces icon is visible with hover tooltip "Interfaces"
- [ ] Load `#/interfaces` with no interfaces configured in config; empty state message is displayed
- [ ] On mobile, open sidebar and tap "Interfaces"; sidebar closes, Interfaces page renders
