use std::future::Future;
use std::pin::Pin;
use std::process::Command;

use super::{PermissionLevel, Tool, ToolError};

/// Skill that extracts transcripts from YouTube videos using yt-dlp.
pub struct YouTubeTranscriptSkill;

impl YouTubeTranscriptSkill {
    pub fn new() -> Self {
        Self
    }
}

/// Extract video ID from any YouTube URL format (watch?v=, youtu.be/, shorts/, or bare ID)
fn extract_video_id(url: &str) -> String {
    // Try youtube.com/watch?v=ID
    if let Some(pos) = url.find("v=") {
        let vid = &url[pos + 2..];
        let vid = vid.split('&').next().unwrap_or(vid);
        if !vid.is_empty() && vid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return vid.to_string();
        }
    }

    // Try youtu.be/ID
    if let Some(pos) = url.rfind('/') {
        let vid = &url[pos + 1..];
        if !vid.is_empty() && vid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return vid.to_string();
        }
    }

    // Try youtube.com/shorts/ID
    if let Some(pos) = url.rfind('/') {
        let part = &url[pos + 1..];
        if !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return part.to_string();
        }
    }

    // Fallback: return as-is (shouldn't happen if validation passed)
    url.to_string()
}

/// Validate that the input looks like a YouTube URL or video ID.
fn validate_youtube_input(input: &str) -> Result<String, ToolError> {
    let input = input.trim();

    // Check if it's a bare video ID (11 characters, alphanumeric + hyphen/underscore)
    if input.len() == 11 && input.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Ok(format!("https://youtube.com/watch?v={}", input));
    }

    // Check if it's a valid YouTube URL
    // youtube.com/watch?v=XXX
    if input.contains("youtube.com/watch?v=") {
        // Extract the video ID
        if let Some(pos) = input.find("v=") {
            let vid = &input[pos + 2..];
            let vid = vid.split('&').next().unwrap_or(vid);
            if vid.len() == 11 && vid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Ok(input.to_string());
            }
        }
    }

    // youtu.be/XXX
    if input.contains("youtu.be/") {
        if let Some(pos) = input.rfind('/') {
            let vid = &input[pos + 1..];
            if vid.len() == 11 && vid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Ok(input.to_string());
            }
        }
    }

    // youtube.com/shorts/XXX
    if input.contains("youtube.com/shorts/") {
        if let Some(pos) = input.rfind('/') {
            let vid = &input[pos + 1..];
            if vid.len() == 11 && vid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Ok(input.to_string());
            }
        }
    }

    Err(ToolError::InvalidInput(
        "invalid YouTube URL or video ID. Expected formats: https://youtube.com/watch?v=XXX, https://youtu.be/XXX, or bare 11-character video ID".into()
    ))
}

/// Check if yt-dlp is installed and available.
fn check_yt_dlp_installed() -> Result<String, ToolError> {
    let path = get_yt_dlp_command();
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::ExecutionFailed(
                    "yt-dlp not found. Please install yt-dlp.".into()
                )
            } else {
                ToolError::ExecutionFailed(format!("failed to run yt-dlp: {e}"))
            }
        })?;

    if !output.status.success() {
        return Err(ToolError::ExecutionFailed(
            "yt-dlp is installed but not working correctly".into()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the yt-dlp command to use (path or just "yt-dlp")
fn get_yt_dlp_command() -> &'static str {
    // Check various possible locations
    let paths = [
        "/home/zoltesh/bin/yt-dlp",
        "/home/zoltesh/yt-dlp",
        "yt-dlp",
    ];
    for path in &paths {
        if std::path::Path::new(path).exists() {
            return path;
        }
    }
    "yt-dlp"
}

/// Parse VTT subtitle file to plain text, removing timestamps and tags.
fn parse_vtt_to_text(vtt: &str) -> String {
    let mut result = String::new();
    let mut in_header = true;

    for line in vtt.lines() {
        let line = line.trim();

        // Skip WEBVTT header and blank lines at start
        if in_header {
            if line.is_empty() {
                in_header = false;
            }
            if line.starts_with("WEBVTT") || line.starts_with("NOTE") {
                continue;
            }
            continue;
        }

        // Skip timestamp lines (00:00:00.000 --> 00:00:00.000)
        if line.contains("-->") {
            continue;
        }

        // Skip cue identifiers (numbers at start of lines)
        if line.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        // Remove HTML-like tags <c>, </c>, <b>, </b>, <i>, </i>, etc.
        let text = line
            .replace(|c: char| c == '<' || c == '>', ".")  // Replace brackets with dots
            .split(|c: char| !c.is_ascii_alphanumeric() && c != ' ' && c != '.')
            .filter(|s| !s.is_empty() && s.chars().any(|c| c.is_alphabetic()))
            .collect::<Vec<_>>()
            .join(" ");

        if !text.is_empty() {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&text);
        }
    }

    // Clean up multiple spaces
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Tool for YouTubeTranscriptSkill {
    fn name(&self) -> &str {
        "youtube_transcript"
    }

    fn description(&self) -> &str {
        "Extract the transcript/subtitles from a YouTube video"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Network
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "video_url": {
                    "type": "string",
                    "description": "YouTube video URL or video ID (e.g., https://youtube.com/watch?v=XXX or just XXX)"
                },
                "chunk_index": {
                    "type": "integer",
                    "description": "Zero-based index of the chunk to retrieve (optional, for long transcripts)"
                },
                "chunk_size": {
                    "type": "integer",
                    "description": "Number of words per chunk (default: 1000)"
                }
            },
            "required": ["video_url"]
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let video_url = input
                .get("video_url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing required field: video_url".into()))?;

            // Parse optional chunk parameters
            let chunk_index = input
                .get("chunk_index")
                .and_then(|v| v.as_i64())
                .map(|v| v as usize);
            let chunk_size = input
                .get("chunk_size")
                .and_then(|v| v.as_i64())
                .map(|v| v as usize)
                .unwrap_or(1000);

            // Validate chunk_size
            if chunk_size == 0 {
                return Err(ToolError::InvalidInput("chunk_size must be greater than 0".into()));
            }

            // Validate chunk_index if provided
            if let Some(idx) = chunk_index {
                if idx > 0 {
                    // Will validate after we know total chunks
                }
            }

            // Validate and normalize the YouTube URL
            let video_url = validate_youtube_input(video_url)?;

            // Check if yt-dlp is available
            let _version = check_yt_dlp_installed()?;

            // Create temp directory for subtitle file
            // Extract video ID from the normalized URL (works for all formats: watch?v=, youtu.be/, shorts/)
            let temp_dir = std::env::temp_dir();
            let video_id = extract_video_id(&video_url);
            let subtitle_file = temp_dir.join(format!("{}.en.vtt", video_id));

            // Run yt-dlp to extract transcript to a file
            let yt_dlp = get_yt_dlp_command();
            let output = Command::new(yt_dlp)
                .args([
                    "--write-subs",
                    "--write-auto-subs",
                    "--skip-download",
                    "--sub-lang", "en",
                    "--convert-subs", "vtt",
                    "-o", &temp_dir.join("%(id)s").to_string_lossy(),
                    &video_url
                ])
                .output()
                .map_err(|e| ToolError::ExecutionFailed(format!("failed to run yt-dlp: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Check for common error cases
                if stderr.contains("Video unavailable") {
                    return Err(ToolError::ExecutionFailed(
                        "video is unavailable or private".into()
                    ));
                }
                if stderr.contains("Could not find subtitles") || stderr.contains("No subtitles were found") || stderr.contains("No captions were found") {
                    return Err(ToolError::ExecutionFailed(
                        "this video does not have subtitles or transcripts available".into()
                    ));
                }
                if stderr.contains("age") || stderr.contains("age-restricted") {
                    return Err(ToolError::ExecutionFailed(
                        "this video is age-restricted and cannot be accessed".into()
                    ));
                }

                return Err(ToolError::ExecutionFailed(format!(
                    "yt-dlp failed: {}",
                    stderr.trim()
                )));
            }

            // Read the subtitle file
            let raw_transcript = if subtitle_file.exists() {
                std::fs::read_to_string(&subtitle_file)
                    .map_err(|e| ToolError::ExecutionFailed(format!("failed to read transcript: {e}")))?
            } else {
                // Try alternative filename patterns
                let alt_pattern = temp_dir.join(format!("{}*.vtt", video_id));
                if let Ok(matches) = glob::glob(&alt_pattern.to_string_lossy()) {
                    if let Some(Ok(path)) = matches.into_iter().next() {
                        std::fs::read_to_string(&path)
                            .map_err(|e| ToolError::ExecutionFailed(format!("failed to read transcript: {}", e)))?
                    } else {
                        return Err(ToolError::ExecutionFailed(
                            "no transcript file found".into()
                        ));
                    }
                } else {
                    return Err(ToolError::ExecutionFailed(
                        "no transcript file found".into()
                    ));
                }
            };

            // Clean up subtitle file
            let _ = std::fs::remove_file(&subtitle_file);

            if raw_transcript.is_empty() {
                return Err(ToolError::ExecutionFailed(
                    "no transcript found for this video".into()
                ));
            }

            // Parse VTT to plain text (remove timestamps and tags)
            let transcript = parse_vtt_to_text(&raw_transcript);
            let word_count = transcript.split_whitespace().count();
            let total_chunks = (word_count + chunk_size - 1) / chunk_size; // ceil division

            // Handle chunking
            let (chunk_text, current_chunk) = if let Some(idx) = chunk_index {
                // Validate chunk_index
                if idx >= total_chunks {
                    return Err(ToolError::InvalidInput(format!(
                        "chunk_index {} out of range (total chunks: {})",
                        idx, total_chunks
                    )));
                }

                let words: Vec<&str> = transcript.split_whitespace().collect();
                let start = idx * chunk_size;
                let end = std::cmp::min(start + chunk_size, words.len());
                let chunk_text = words[start..end].join(" ");
                (chunk_text, idx)
            } else {
                // No chunk requested - return full transcript with chunk info
                (transcript, 0)
            };

            // Build response
            let mut response = serde_json::json!({
                "transcript": chunk_text,
                "video_url": video_url,
                "word_count": word_count,
                "total_chunks": total_chunks,
                "current_chunk": current_chunk,
                "chunk_size": chunk_size
            });

            // Add has_more flag if there are more chunks
            if current_chunk < total_chunks - 1 {
                response["has_more"] = serde_json::json!(true);
            }

            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_youtube_input_accepts_watch_url() {
        let result = validate_youtube_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    }

    #[test]
    fn validate_youtube_input_accepts_youtu_be() {
        let result = validate_youtube_input("https://youtu.be/dQw4w9WgXcQ");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_youtube_input_accepts_bare_id() {
        let result = validate_youtube_input("dQw4w9WgXcQ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://youtube.com/watch?v=dQw4w9WgXcQ");
    }

    #[test]
    fn validate_youtube_input_rejects_invalid() {
        let result = validate_youtube_input("not-a-youtube-url");
        assert!(result.is_err());
    }

    #[test]
    fn validate_youtube_input_rejects_short_id() {
        let result = validate_youtube_input("short");
        assert!(result.is_err());
    }

    #[test]
    fn validate_youtube_input_accepts_shorts() {
        let result = validate_youtube_input("https://www.youtube.com/shorts/dQw4w9WgXcQ");
        assert!(result.is_ok());
    }

    #[test]
    fn video_id_extraction_from_normalized_url() {
        // Test that after validation, we can extract the video ID correctly
        // Test various URL formats directly (not normalized)
        assert_eq!(extract_video_id("https://youtu.be/dQw4w9WgXcQ"), "dQw4w9WgXcQ");
        assert_eq!(extract_video_id("https://youtube.com/watch?v=abc123"), "abc123");
        assert_eq!(extract_video_id("https://youtube.com/watch?v=abc123&t=60"), "abc123");
        assert_eq!(extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"), "dQw4w9WgXcQ");
        assert_eq!(extract_video_id("rXfFACs24zU"), "rXfFACs24zU");
    }
}
