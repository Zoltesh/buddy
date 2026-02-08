# 040 — Application Shell & Navigation

## Description

The current sidebar is a conversation list with a settings link bolted onto the bottom. It works for two routes, but it conflates application navigation with conversation management — two concerns that diverge as buddy grows. This task restructures the sidebar into a proper application shell: a collapsible navigation rail for route switching, with a contextual panel below it for route-specific content (conversation list when on Chat, empty when on Settings). This establishes the navigation surface that V0.5 extends for multi-interface management and V0.6 for agent orchestration.

## Goal

The application has a professional, collapsible sidebar that cleanly separates primary navigation from route-specific content, scaling to support future feature surfaces without layout changes.

## Requirements

- Restructure `Sidebar.svelte` into two visual zones:
  1. **Navigation rail** — Icon + label buttons for each primary route (Chat, Settings). Always visible in both expanded and collapsed states.
  2. **Context panel** — Content below the rail that changes with the active route. When Chat is active: "New Chat" button and conversation list. When Settings is active: empty (future tasks may add section quick-links).
- **Brand identity:** A compact "buddy" wordmark or icon mark at the top of the sidebar, above the navigation rail. Visible in both expanded and collapsed states (wordmark can shorten to icon when collapsed).
- **Active route indication:** The active nav item has a distinct highlighted background and a left accent border. Inactive items have a subtle hover state.
- **Collapsible (desktop only):**
  - Expanded (default): ~256px wide. Icons, labels, and context panel all visible.
  - Collapsed: ~56px wide icon rail. Icons only, no labels, no context panel. Hover tooltips on nav icons show the label.
  - Toggle control at the bottom of the sidebar (e.g., a chevron or collapse icon).
  - Collapse preference stored in `localStorage`; restored on page load.
- **Mobile (< md breakpoint):** Overlay behavior is unchanged — hamburger button opens the sidebar as a full-width slide-out with backdrop. The collapse rail is not used on mobile; the sidebar is always expanded when open.
- **Transitions:** Sidebar width, label/context panel opacity, and icon sizing use CSS transitions (~200ms, `transition-all` or scoped properties).
- **Keyboard accessible:** Nav items are focusable, activatable via Enter/Space, and have visible focus rings.
- **No backend changes.** This is a frontend-only task.

## Acceptance Criteria

- [ ] The sidebar has a navigation section with Chat and Settings as distinct icon+label items
- [ ] Clicking a nav item navigates to the corresponding route and highlights it as active
- [ ] The conversation list renders below navigation only when the Chat route is active
- [ ] A "buddy" brand mark is visible at the top of the sidebar in both expanded and collapsed states
- [ ] On desktop, a toggle collapses the sidebar to an icon-only rail (~56px)
- [ ] In collapsed state, nav icons are visible and functional with hover tooltips
- [ ] In collapsed state, labels and the context panel (conversation list) are hidden
- [ ] Collapse/expand preference persists across page reloads via localStorage
- [ ] On mobile, the sidebar opens as a full-width overlay (existing behavior preserved)
- [ ] Transitions between expanded and collapsed states are smooth (~200ms)
- [ ] All existing functionality works: new chat, conversation switching, conversation deletion, settings navigation, mobile hamburger toggle

## Test Cases

- [ ] Load the app on desktop; sidebar is expanded with a "buddy" brand mark at top, Chat and Settings nav items visible, conversation list below navigation
- [ ] Click the Settings nav item; URL changes to `#/settings`, Settings item is highlighted, Chat item is not, conversation list is no longer visible in sidebar
- [ ] Click the Chat nav item from the Settings page; URL changes to `#/`, Chat item is highlighted, conversation list reappears
- [ ] Click the collapse toggle; sidebar shrinks to icon rail, labels and conversation list are hidden, nav icons remain visible and clickable
- [ ] In collapsed state, hover a nav icon; a tooltip showing the label appears
- [ ] In collapsed state, click the Settings icon; route changes to `#/settings`
- [ ] Click the expand toggle; sidebar returns to full width with labels and context panel
- [ ] Collapse sidebar, reload the page; sidebar loads collapsed (localStorage)
- [ ] Expand sidebar, reload the page; sidebar loads expanded (localStorage)
- [ ] Resize browser below md breakpoint; hamburger button appears, collapse toggle is hidden, opening sidebar shows full expanded layout with navigation and conversation list
- [ ] On mobile, tap hamburger to open sidebar, then tap Settings nav item; sidebar closes, settings page renders
