# 080 — Consolidate Duplicate Status Functions in Interfaces.svelte

## Description

`frontend/src/lib/Interfaces.svelte` (lines 69-103) has three functions — `statusLabel()`, `statusColor()`, `statusTextColor()` — that each repeat the same conditional chain (`isConfigured` → `isEnabled` → `healthChecking` → `healthResult`). This duplicated logic means any change to the status model requires updating three places.

## Goal

Replace the three duplicate functions with a single function that returns all status display properties at once.

## Requirements

- Create a single function (e.g., `getStatus(name)`) that returns an object with all three properties:
  ```javascript
  function getStatus(name) {
    if (!isConfigured(name)) return { label: 'Not configured', color: 'bg-gray-400', textColor: 'text-gray-500 dark:text-gray-400' };
    if (!isEnabled(name))   return { label: 'Disabled',       color: 'bg-gray-400', textColor: 'text-gray-500 dark:text-gray-400' };
    if (healthChecking[name]) return { label: 'Checking...',   color: 'bg-yellow-400 animate-pulse', textColor: 'text-yellow-600 dark:text-yellow-400' };
    const hr = healthResult[name];
    if (hr) {
      const connected = hr.status === 'connected';
      return {
        label: connected ? `Connected — ${hr.detail}` : hr.detail,
        color: connected ? 'bg-green-500' : 'bg-red-500',
        textColor: connected ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400',
      };
    }
    return { label: 'Enabled', color: 'bg-gray-400', textColor: 'text-gray-500 dark:text-gray-400' };
  }
  ```
- Remove the three individual functions: `statusLabel()`, `statusColor()`, `statusTextColor()`
- Update all template references to use the new function. For example, if the template currently has:
  ```html
  <span class="{statusColor(name)}">{statusLabel(name)}</span>
  ```
  Change to something like:
  ```html
  {@const status = getStatus(name)}
  <span class="{status.color}">{status.label}</span>
  ```
  Or use a `$derived` if appropriate.
- The rendered output must be pixel-identical — same classes, same text, same behavior.

## Files to Modify

- `frontend/src/lib/Interfaces.svelte` — replace 3 functions with 1, update template

## Acceptance Criteria

- [ ] Only one function computes interface status (not three)
- [ ] The conditional logic appears exactly once
- [ ] The rendered Interfaces page looks and behaves identically to before
- [ ] All existing tests pass

## Test Cases

- [ ] Open the Interfaces page with a configured and enabled Telegram interface; assert status shows correctly
- [ ] Open the Interfaces page with an unconfigured interface; assert "Not configured" appears with gray styling
- [ ] Disable an interface; assert "Disabled" appears with correct styling
- [ ] Run a health check; assert "Checking..." appears with yellow pulse, then resolves to Connected/error
