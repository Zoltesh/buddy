# XSS Sanitization for Markdown Output

**Date:** 2026-02-16
**Task:** 071-sanitize-markdown-output
**Status:** Design approved

## Problem

`Chat.svelte` renders LLM responses as HTML using `{@html marked.parse(content)}` with no sanitization. The `marked` library does not sanitize HTML by default. If an LLM response contains malicious HTML (`<script>` tags, `<img onerror=...>`, etc.), it executes directly in the DOM.

**Attack vector:** LLM echoes malicious user input from Telegram/WhatsApp, or generates malicious content. Combined with auth tokens in localStorage, this enables session hijacking.

## Design Decision

**Approach:** Whitelist-only DOMPurify sanitization

Install DOMPurify and wrap `marked.parse()` output with strict whitelist configuration. Only allow HTML tags that markdown generates (headings, bold, italic, lists, code blocks, links, tables). Block all raw HTML, even "safe" tags like `<div>`.

**Rationale:** Since this is an LLM chat interface, there's no legitimate reason for raw HTML beyond markdown-generated tags. Explicit whitelisting provides the strongest security guarantee and eliminates entire classes of bypasses.

## Implementation

### 1. Add DOMPurify Dependency

```bash
cd frontend && npm install dompurify
```

Add to `frontend/package.json` dependencies:
```json
"dompurify": "^3.2.2"
```

### 2. Update Chat.svelte

**Import DOMPurify:**
```javascript
import DOMPurify from 'dompurify';
```

**Update renderMarkdown() function (line 228-230):**
```javascript
function renderMarkdown(content) {
  const rawHtml = marked.parse(content);
  return DOMPurify.sanitize(rawHtml, {
    ALLOWED_TAGS: [
      'p', 'br', 'strong', 'em', 'code', 'pre', 'a', 'ul', 'ol', 'li',
      'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'blockquote', 'table',
      'thead', 'tbody', 'tr', 'th', 'td', 'hr', 'del', 'sup', 'sub'
    ],
    ALLOWED_ATTR: ['href', 'class'],
    ALLOW_DATA_ATTR: false
  });
}
```

**What this blocks:**
- `<script>` tags
- Inline event handlers (`onerror`, `onclick`, `onload`, etc.)
- `javascript:` URLs in links (DOMPurify strips these automatically)
- `<iframe>`, `<object>`, `<embed>` tags
- Any HTML tag not in the whitelist

**What this preserves:**
- Markdown formatting: headings, bold, italic, code blocks, lists, tables, links
- `class` attribute: needed for Tailwind prose styles
- `href` attribute: needed for links

### 3. No Changes to marked Configuration

Keep `marked.setOptions({ breaks: true, gfm: true })` as-is. Sanitization happens after markdown parsing.

## Testing

### Test Cases (from task spec)

Add Rust integration tests in `buddy-server/src/api/tests.rs`:

1. **Markdown still works:** Send `**bold** and _italic_` → verify response contains `<strong>` and `<em>` tags
2. **Script tags blocked:** Send `<script>alert('xss')</script>` → verify no `<script>` in response
3. **Event handlers blocked:** Send `<img src=x onerror=alert('xss')>` → verify no `onerror` attribute
4. **Code blocks safe:** Send fenced code block with HTML → verify renders as code, not executable
5. **JavaScript URLs blocked:** Send `[link](javascript:alert('xss'))` → verify no `javascript:` in href

### Test Implementation Strategy

- Use existing `post_chat()` and `parse_sse_events()` helpers from testutil
- Send malicious payloads via `/api/chat` endpoint
- Parse SSE response and verify sanitized output
- Test end-to-end (backend → frontend rendering) to verify complete threat model

## Security Properties

- **Defense in depth:** Even if marked has a bug, whitelist prevents unknown tags
- **No bypass vectors:** Explicitly listing allowed tags eliminates entire attack surface
- **Future-proof:** New XSS vectors in HTML spec won't affect us (not in whitelist)
- **Stable:** Markdown output structure hasn't changed in years; tag list won't need updates

## Files Modified

- `frontend/package.json` — add `dompurify` dependency
- `frontend/src/lib/Chat.svelte` — import DOMPurify, update `renderMarkdown()`
- `buddy-server/src/api/tests.rs` — add XSS sanitization test cases

## Acceptance Criteria

- [ ] `dompurify` listed in `frontend/package.json` dependencies
- [ ] `renderMarkdown()` returns sanitized HTML with whitelist config
- [ ] Normal markdown (headings, bold, code blocks, links, lists, tables) renders correctly
- [ ] `<script>alert('xss')</script>` does not execute
- [ ] `<img src=x onerror=alert('xss')>` does not execute
- [ ] `[link](javascript:alert('xss'))` does not contain `javascript:` URL
- [ ] All existing tests pass
- [ ] All new test cases pass
