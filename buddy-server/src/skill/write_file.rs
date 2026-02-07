use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use crate::config::WriteFileConfig;

use super::{PermissionLevel, Skill, SkillError, normalize_path};

/// Skill that writes file contents to sandboxed directories.
pub struct WriteFileSkill {
    allowed_directories: Vec<PathBuf>,
}

impl WriteFileSkill {
    pub fn new(config: &WriteFileConfig) -> Self {
        Self {
            allowed_directories: config.allowed_directories.iter().map(PathBuf::from).collect(),
        }
    }
}

/// Validate that a target write path is within an allowed directory.
///
/// 1. Normalize the path (resolve `..` without filesystem access)
/// 2. Check against canonicalized allowed directories
/// 3. Create parent directories
/// 4. Canonicalize the parent to catch symlink attacks, re-verify
fn validate_write_path(
    path: &str,
    allowed_dirs: &[PathBuf],
) -> Result<PathBuf, SkillError> {
    let normalized = normalize_path(Path::new(path))?;

    // First pass: check normalized path against allowed dirs
    let mut allowed = false;
    for dir in allowed_dirs {
        let canonical_dir = std::fs::canonicalize(dir).map_err(|e| {
            SkillError::ExecutionFailed(format!(
                "cannot resolve allowed directory '{}': {e}",
                dir.display()
            ))
        })?;
        if normalized.starts_with(&canonical_dir) {
            allowed = true;
            break;
        }
    }

    if !allowed {
        return Err(SkillError::Forbidden(format!(
            "path '{path}' is outside allowed directories"
        )));
    }

    // Create parent directories
    if let Some(parent) = normalized.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SkillError::ExecutionFailed(format!("failed to create parent directories: {e}"))
        })?;
    }

    // Second pass: canonicalize parent to catch symlink-based escapes
    let parent = normalized
        .parent()
        .ok_or_else(|| SkillError::InvalidInput("path has no parent directory".into()))?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|e| SkillError::ExecutionFailed(format!("cannot resolve parent directory: {e}")))?;
    let final_path = canonical_parent.join(
        normalized
            .file_name()
            .ok_or_else(|| SkillError::InvalidInput("path has no file name".into()))?,
    );

    let mut verified = false;
    for dir in allowed_dirs {
        let canonical_dir = std::fs::canonicalize(dir).map_err(|e| {
            SkillError::ExecutionFailed(format!(
                "cannot resolve allowed directory '{}': {e}",
                dir.display()
            ))
        })?;
        if final_path.starts_with(&canonical_dir) {
            verified = true;
            break;
        }
    }

    if !verified {
        return Err(SkillError::Forbidden(format!(
            "path '{path}' resolves outside allowed directories"
        )));
    }

    Ok(final_path)
}

impl Skill for WriteFileSkill {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file in an allowed directory"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Mutating
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to write" },
                "content": { "type": "string", "description": "Content to write to the file" }
            },
            "required": ["path", "content"]
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidInput("missing required field: path".into()))?;
            let content = input
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SkillError::InvalidInput("missing required field: content".into())
                })?;

            let resolved = validate_write_path(path, &self.allowed_directories)?;

            let bytes = content.len();
            std::fs::write(&resolved, content)
                .map_err(|e| SkillError::ExecutionFailed(format!("failed to write file: {e}")))?;

            Ok(serde_json::json!({ "bytes_written": bytes }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(dirs: &[&str]) -> WriteFileSkill {
        WriteFileSkill {
            allowed_directories: dirs.iter().map(PathBuf::from).collect(),
        }
    }

    #[tokio::test]
    async fn write_file_inside_allowed_dir() {
        let dir = std::env::temp_dir().join("buddy-test-write-ok");
        std::fs::create_dir_all(&dir).unwrap();

        let file = dir.join("output.txt");
        let skill = make_skill(&[dir.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({
                "path": file.to_str().unwrap(),
                "content": "written by buddy"
            }))
            .await
            .unwrap();

        assert_eq!(result["bytes_written"], 16);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "written by buddy");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn write_file_creates_parent_directories() {
        let dir = std::env::temp_dir().join("buddy-test-write-parents");
        std::fs::create_dir_all(&dir).unwrap();

        let file = dir.join("sub").join("dir").join("deep.txt");
        let skill = make_skill(&[dir.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({
                "path": file.to_str().unwrap(),
                "content": "deep write"
            }))
            .await
            .unwrap();

        assert_eq!(result["bytes_written"], 10);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "deep write");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn write_file_outside_allowed_dir_is_forbidden() {
        let allowed = std::env::temp_dir().join("buddy-test-write-sandbox");
        std::fs::create_dir_all(&allowed).unwrap();

        let outside = std::env::temp_dir().join("buddy-test-write-outside");
        let file = outside.join("nope.txt");

        let skill = make_skill(&[allowed.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({
                "path": file.to_str().unwrap(),
                "content": "nope"
            }))
            .await;

        match result {
            Err(SkillError::Forbidden(_)) => {}
            other => panic!("expected Forbidden, got {other:?}"),
        }

        std::fs::remove_dir_all(&allowed).ok();
    }

    #[tokio::test]
    async fn write_file_with_traversal_is_forbidden() {
        let allowed = std::env::temp_dir().join("buddy-test-write-traversal");
        std::fs::create_dir_all(&allowed).unwrap();

        let path = format!("{}/../escape.txt", allowed.to_str().unwrap());
        let skill = make_skill(&[allowed.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({
                "path": path,
                "content": "escape"
            }))
            .await;

        match result {
            Err(SkillError::Forbidden(_)) => {}
            other => panic!("expected Forbidden, got {other:?}"),
        }

        std::fs::remove_dir_all(&allowed).ok();
    }
}
