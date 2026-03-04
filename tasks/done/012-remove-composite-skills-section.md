# Remove Composite Skills Section from UI

## Description

Remove the "Composite Skills" section from the Skills tab UI. The skills (remember, recall) should appear directly in the main Skills section without the confusing "composite" labeling.

## Requirements

- Remove the "Composite Skills" section from SkillsTab.svelte
- Move the remember and recall skill cards to appear directly in the main Skills section
- The term "composite" is redundant since all skills are composite by definition

## Files to Modify

- `frontend/src/lib/settings/SkillsTab.svelte` — Remove compositeSkills section, integrate skills directly

## Dependencies

- Task 011: Fix UI - Move Tools Out of Skills Tab (can be done in parallel)

## Acceptance Criteria

- [x] No "Composite Skills" section in UI
- [x] remember and recall appear in main Skills section
- [x] UI is cleaner and less confusing

## Test Cases

- [x] Verify Skills tab displays correctly with skills in main section
