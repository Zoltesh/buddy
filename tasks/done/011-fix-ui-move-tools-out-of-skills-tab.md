# Fix UI: Move Tools Out of Skills Tab

## Description

Fix the UI bug where read_file, write_file, and fetch_url are incorrectly listed as "Skills" in the Skills tab. According to the tools/skills architecture, these are atomic tools, not skills.

## Requirements

- Remove read_file, write_file, and fetch_url from the Skills tab
- Keep only actual skills (remember, recall) in the Skills tab
- The Tools tab should be the only place to configure these atomic operations

## Files to Modify

- `frontend/src/lib/settings/SkillsTab.svelte` — Remove tool definitions from skillDefs array

## Dependencies

- Task 008: Update Frontend to use ToolsConfig (for proper config reference)

## Acceptance Criteria

- [x] Skills tab only shows actual skills (remember, recall)
- [x] read_file, write_file, fetch_url only appear in Tools tab
- [x] UI correctly reflects tools/skills architecture

## Test Cases

- [x] Verify Skills tab shows only remember and recall
- [x] Verify Tools tab shows all tools
