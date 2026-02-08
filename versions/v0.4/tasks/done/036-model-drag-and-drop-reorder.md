# 036 — Model Provider Drag-and-Drop Reordering

## Description

Add drag-and-drop vertical reordering to the provider lists in the Models settings section. The order of providers determines fallback priority — the topmost provider is the default, and the rest are tried in order if it fails.

## Goal

Users can intuitively reorder their model providers by dragging them, controlling fallback priority without editing config files.

## Requirements

- Implement drag-and-drop reordering on the provider lists in both the Chat and Embedding sub-sections
- Visual feedback during drag:
  - The dragged item has a "lifted" appearance (slight shadow/scale, reduced opacity at the original position)
  - A drop indicator (line or highlighted gap) shows where the item will be placed
  - The list reorders in real-time as the user drags over different positions
- On drop, the new order is persisted by sending `PUT /api/config/models` with the reordered provider array
- The position labels (primary, fallback #2, etc.) update immediately after reorder
- A drag handle icon on each provider card signals that items are draggable
- Touch support for mobile devices (use pointer events or a library that handles both mouse and touch)
- If saving fails, revert to the previous order and show an error message
- Use native HTML5 Drag and Drop API or a lightweight library — avoid heavy dependencies

## Acceptance Criteria

- [x] Providers can be reordered by dragging and dropping
- [x] Visual feedback indicates the dragged item and the drop target position
- [x] The new order is persisted via `PUT /api/config/models` on drop
- [x] Position labels update to reflect the new order
- [x] Drag handles are visible on each provider card
- [x] Failed saves revert the list to the previous order with an error message
- [x] Drag-and-drop works on both desktop (mouse) and mobile (touch)

## Test Cases

- [x] Drag the second provider to the first position; assert `PUT /api/config/models` is called with the reversed order
- [x] After reorder, assert position labels update (the dragged item shows "Primary")
- [x] Simulate a save failure after reorder; assert the list reverts to the original order
- [x] Assert the drag handle is visible on each provider card
- [x] Drag a provider within a single-item list (no-op); assert no API call is made
- [x] Reorder chat providers; assert embedding providers are unchanged in the saved config
