# 038 — Settings Status Indicators and Validation

## Description

Add visual status indicators to the Settings page that highlight configuration problems. Red indicators for critical issues (required model slot empty, all providers unreachable) and yellow indicators for warnings (single provider with no fallback). Connection status is checked on page load.

## Goal

Users can see at a glance whether their configuration is healthy, degraded, or broken — directly in the Settings UI — and know exactly what to fix.

## Requirements

- On Settings page load, after fetching the config, run a health check:
  - For each configured provider, call `POST /api/config/test-provider` to verify reachability
  - Display results inline next to each provider: green checkmark (reachable), red X (unreachable), spinner (testing)
  - Run tests in parallel (do not block on each one sequentially)
- Section-level status indicators:
  - **Chat Models:**
    - Red indicator if no chat providers are configured
    - Red indicator if all chat providers are unreachable
    - Yellow indicator if only one provider is configured (no fallback)
    - Green indicator if at least one provider is reachable
  - **Embedding Models:**
    - Yellow indicator if no embedding providers are configured (memory features disabled)
    - Red indicator if configured but all unreachable
    - Green indicator if at least one is reachable
  - **Skills:**
    - Yellow indicator if a skill is enabled but has empty sandbox rules (no allowed directories/domains)
- Inline field validation (in addition to server-side validation from task 030):
  - Required fields highlighted with a red border when empty and the form has been submitted
  - Validation messages appear below the field
  - Validation runs on blur and on submit
- Status indicators use color + icon (not color alone) for accessibility
- A "Recheck" button re-runs all provider connectivity tests

## Acceptance Criteria

- [ ] Provider connectivity is tested on Settings page load
- [ ] Each provider shows a green/red status icon based on reachability
- [ ] Chat section shows a red indicator when no providers are configured
- [ ] Chat section shows a red indicator when all providers are unreachable
- [ ] Chat section shows a yellow indicator for single-provider-no-fallback
- [ ] Embedding section shows a yellow indicator when not configured
- [ ] Skills section shows a yellow indicator for empty sandbox rules
- [ ] Inline field validation highlights required fields and shows error messages
- [ ] Status indicators use icon + color (accessible)
- [ ] "Recheck" button re-runs connectivity tests

## Test Cases

- [ ] Load settings with a reachable provider; assert green checkmark appears next to it
- [ ] Load settings with an unreachable provider; assert red X appears next to it
- [ ] Load settings with no chat providers; assert a red section-level indicator on the Chat Models section
- [ ] Load settings with one chat provider; assert a yellow section-level indicator (no fallback)
- [ ] Load settings with no embedding config; assert a yellow indicator on the Embedding section
- [ ] Load settings with a skill that has empty allowed_directories; assert a yellow indicator on the Skills section
- [ ] Submit a form with an empty required field; assert a red border and error message appear
- [ ] Click "Recheck"; assert connectivity tests run again and indicators update
