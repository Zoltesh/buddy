# Fix YouTube Video ID Extraction Bug in Transcript Tool

## Feature
- Feature ID: F-20260305-001-youtube-tools
- PRD: N/A (bug fix)

## Description
Fix the video ID extraction logic in `youtube_transcript.rs` that fails to properly extract the video ID from `youtu.be` URLs, causing "no transcript file found" errors.

## Context
When a user provides a YouTube URL in the format `https://youtu.be/rXfFACs24zU`, the code at line 228 uses:
```rust
let video_id = video_url.split('=').last().unwrap_or(&video_url);
```

This fails because there's no `=` in youtu.be URLs, so it returns the entire URL instead of just the video ID. The transcript file is saved by yt-dlp as `rXfFACs24zU.en.vtt` but the code looks for `https://youtu.be/rXfFACs24zU.en.vtt`.

The `validate_youtube_input` function correctly normalizes URLs to `https://youtube.com/watch?v=XXX` format, but the execute function doesn't reuse this normalized URL to extract the video ID properly.

## Dependencies
- None (standalone fix)

## Files / Areas
- `buddy-core/src/skill/youtube_transcript.rs` (lines 228-229)

## Acceptance Criteria
- [x] URLs like `https://youtu.be/rXfFACs24zU` correctly extract video ID `rXfFACs24zU`
- [x] URLs like `https://youtube.com/watch?v=XXX` continue to work
- [x] Bare video IDs (e.g., `rXfFACs24zU`) continue to work
- [x] Unit test added for video ID extraction from youtu.be URLs

## Test Cases / Validation
- [x] Run: `cargo test -p buddy-core youtube_transcript -- --nocapture` - 7 tests pass
- [x] Verify: Added `video_id_extraction_from_normalized_url` test
- [x] Run: `cargo build -p buddy-core` - builds successfully
- [ ] Manual test: Use UI to summarize `https://youtu.be/rXfFACs24zU` - transcript retrieves successfully

## Notes
- Clippy warnings are pre-existing (not from this change)
