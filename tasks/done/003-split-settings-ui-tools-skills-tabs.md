# Split Settings UI into Tools and Skills Tabs

## Description

The current Settings UI has a Skills tab that shows all skills. This needs to be split into two separate tabs: one for Tools (atomic capabilities) and one for Skills (composite operations).

## Goal

Update the Settings UI to have separate Tools and Skills tabs, both read-only for system tools/skills at this phase.

## Requirements

- Create a new "Tools" tab in Settings
- Move current tools to the Tools tab (with descriptions)
- Keep/combine skills in the Skills tab (with descriptions and which tools they use)
- Both tabs are read-only for system tools/skills
- Display tool/schema information so users know what's available

## Files to Modify

- `frontend/src/lib/settings/` — update tab components
- May need new API endpoints to serve tool definitions

## Acceptance Criteria

- [x] New Tools tab displays all available tools with descriptions
- [x] Skills tab displays skills with descriptions and which tools they use
- [x] Both tabs are read-only (system tools/skills)
- [x] UI is consistent with existing design

## Test Cases

- [x] Verify Tools tab shows all tools (read_file, write_file, fetch_url, memory_read, memory_write)
- [x] Verify Skills tab shows skills with tool references
- [x] Verify tabs are navigable and display correctly
