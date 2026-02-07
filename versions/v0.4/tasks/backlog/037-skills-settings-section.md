# 037 — Skills Settings Section

## Description

Build the interactive Skills section of the Settings page. Users can view configured skills, toggle them on/off, and edit their sandbox settings (allowed directories, allowed domains) and approval policies from the browser.

## Goal

Users can manage skill configurations — enabling, disabling, and adjusting sandbox rules and approval policies — entirely from the Settings UI.

## Requirements

- Display each skill (read_file, write_file, fetch_url) as a card or panel with:
  - Skill name and description
  - Enable/disable toggle (a disabled skill has its config section removed from `buddy.toml`; an enabled skill has a config section with at least default values)
  - Current sandbox rules displayed as a list
  - Current approval policy
- **Edit sandbox rules:**
  - `read_file` / `write_file`: editable list of allowed directories
    - Add directory: text input with an "Add" button
    - Remove directory: X button next to each entry
    - Validate that paths are non-empty
  - `fetch_url`: editable list of allowed domains
    - Add domain: text input with an "Add" button
    - Remove domain: X button next to each entry
    - Validate that domains are non-empty
- **Edit approval policy:**
  - Dropdown or radio buttons: "Always ask", "Ask once per conversation", "Trust (auto-approve)"
  - Only shown for skills with `Mutating` or `Network` permission levels
  - `ReadOnly` skills show their permission level but no approval setting
- **Save:** Save button sends the full skills config to `PUT /api/config/skills`
  - Success: confirmation message
  - Failure: show validation errors inline
- When a skill is toggled off, its config section is removed on save. When toggled on, it is initialized with empty allowed lists and the default approval policy.
- Show the skill's permission level as a badge (ReadOnly, Mutating, Network)

## Acceptance Criteria

- [ ] All configurable skills are displayed with their current settings
- [ ] Skills can be toggled on/off
- [ ] Allowed directories can be added and removed for file skills
- [ ] Allowed domains can be added and removed for fetch_url
- [ ] Approval policy can be changed via dropdown for Mutating/Network skills
- [ ] Permission level badges are displayed on each skill
- [ ] Saving calls `PUT /api/config/skills` with the updated config
- [ ] Toggling a skill off removes its config section on save
- [ ] Toggling a skill on initializes it with defaults
- [ ] Validation prevents saving empty paths or domains

## Test Cases

- [ ] Load settings with all three skills configured; assert each displays its name, directories/domains, and approval policy
- [ ] Toggle write_file off; save; assert `PUT /api/config/skills` sends a payload without `write_file`
- [ ] Toggle write_file back on; assert it initializes with an empty allowed_directories list
- [ ] Add a directory to read_file's allowed list; save; assert the new directory appears in the saved config
- [ ] Remove a directory from write_file's allowed list; save; assert it is gone from the saved config
- [ ] Change fetch_url's approval policy to "trust"; save; assert the saved config reflects the change
- [ ] Attempt to save with an empty string in allowed_directories; assert inline validation error
- [ ] Assert ReadOnly skills (read_file) do not show an approval policy dropdown
- [ ] Assert Mutating/Network skills show the approval policy dropdown
