# 071 — Sanitize Markdown Output (XSS Fix)

## Description

`frontend/src/lib/Chat.svelte` renders LLM responses as HTML using `{@html marked.parse(content)}` with no sanitization. The `marked` library does not sanitize HTML by default. If an LLM response contains `<script>` tags, `<img onerror=...>`, or other HTML injection vectors, they render directly into the DOM. Combined with auth tokens stored in `localStorage`, this is a compound XSS vulnerability.

## Goal

All markdown-rendered HTML in the chat UI is sanitized before being inserted into the DOM, preventing any script execution or event handler injection from LLM responses.

## Requirements

- Install `dompurify` as a frontend dependency (`npm install dompurify`)
- In `frontend/src/lib/Chat.svelte`, update the `renderMarkdown()` function (line ~228) to sanitize the output of `marked.parse()` through `DOMPurify.sanitize()` before returning it
- The sanitizer must strip: `<script>` tags, inline event handlers (`onerror`, `onclick`, etc.), `javascript:` URLs, and `<iframe>`/`<object>`/`<embed>` tags
- Markdown formatting (headings, bold, italic, code blocks, links, lists, tables) must continue to render correctly after sanitization
- Do not change `marked.setOptions()` — keep `breaks: true` and `gfm: true`

## Files to Modify

- `frontend/package.json` — add `dompurify` dependency
- `frontend/src/lib/Chat.svelte` — import DOMPurify, wrap `marked.parse()` output in `DOMPurify.sanitize()`

## Acceptance Criteria

- [x] `dompurify` is listed in `frontend/package.json` dependencies
- [x] `renderMarkdown()` returns sanitized HTML
- [x] Normal markdown (headings, bold, code blocks, links, lists) renders correctly
- [x] `<script>alert('xss')</script>` in LLM output does not execute
- [x] `<img src=x onerror=alert('xss')>` in LLM output does not execute
- [x] All existing tests pass

## Test Cases

- [x] Render a message containing `**bold** and _italic_`; assert the output contains `<strong>` and `<em>` tags (markdown still works)
- [x] Render a message containing `<script>alert('xss')</script>`; assert the output does NOT contain `<script>` tags
- [x] Render a message containing `<img src=x onerror=alert('xss')>`; assert the output does NOT contain `onerror`
- [x] Render a message containing a fenced code block with HTML inside it; assert the code block renders as code, not as executable HTML
- [x] Render a message containing `[link](javascript:alert('xss'))`; assert the output does NOT contain `javascript:`
