use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use crate::config::ReadFileConfig;

use super::{Skill, SkillError};

/// Skill that reads file contents from sandboxed directories.
pub struct ReadFileSkill {
    allowed_directories: Vec<PathBuf>,
}

impl ReadFileSkill {
    pub fn new(config: &ReadFileConfig) -> Self {
        Self {
            allowed_directories: config.allowed_directories.iter().map(PathBuf::from).collect(),
        }
    }
}

/// Normalize a path by making it absolute and resolving `.` and `..` without
/// touching the filesystem.
fn normalize_path(path: &Path) -> Result<PathBuf, SkillError> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| SkillError::ExecutionFailed(format!("cannot get current directory: {e}")))?
            .join(path)
    };

    let mut components = Vec::new();
    for component in abs.components() {
        match component {
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            c => components.push(c),
        }
    }

    Ok(components.iter().collect())
}

/// Validate that a path resolves to a location within one of the allowed directories.
///
/// 1. Normalize the path (resolve `..` without filesystem access) and reject if outside sandbox
/// 2. Canonicalize the real path (resolves symlinks) and reject if outside sandbox
fn validate_path(path: &str, allowed_dirs: &[PathBuf]) -> Result<PathBuf, SkillError> {
    // First pass: normalize and check (catches `../` traversal even for non-existent paths)
    let normalized = normalize_path(Path::new(path))?;
    let mut in_sandbox = false;
    for dir in allowed_dirs {
        // Skip allowed directories that don't exist on disk rather than failing.
        let Ok(canonical_dir) = std::fs::canonicalize(dir) else {
            continue;
        };
        if normalized.starts_with(&canonical_dir) {
            in_sandbox = true;
            break;
        }
    }
    if !in_sandbox {
        return Err(SkillError::Forbidden(format!(
            "path '{path}' is outside allowed directories"
        )));
    }

    // Second pass: canonicalize to resolve symlinks and verify again
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| SkillError::ExecutionFailed(format!("cannot resolve path '{path}': {e}")))?;

    for dir in allowed_dirs {
        let canonical_dir = std::fs::canonicalize(dir).unwrap();
        if canonical.starts_with(&canonical_dir) {
            return Ok(canonical);
        }
    }

    Err(SkillError::Forbidden(format!(
        "path '{path}' resolves outside allowed directories"
    )))
}

impl Skill for ReadFileSkill {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file from an allowed directory"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to read" }
            },
            "required": ["path"]
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

            let canonical = validate_path(path, &self.allowed_directories)?;

            let content = std::fs::read_to_string(&canonical)
                .map_err(|e| SkillError::ExecutionFailed(format!("failed to read file: {e}")))?;

            Ok(serde_json::json!({ "content": content }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn make_skill(dirs: &[&str]) -> ReadFileSkill {
        ReadFileSkill {
            allowed_directories: dirs.iter().map(PathBuf::from).collect(),
        }
    }

    #[tokio::test]
    async fn read_file_inside_allowed_dir() {
        let dir = std::env::temp_dir().join("buddy-test-read-ok");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hello.txt");
        std::fs::write(&file, "hello world").unwrap();

        let skill = make_skill(&[dir.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({ "path": file.to_str().unwrap() }))
            .await
            .unwrap();

        assert_eq!(result["content"], "hello world");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_file_with_traversal_is_forbidden() {
        let dir = std::env::temp_dir().join("buddy-test-read-traversal");
        std::fs::create_dir_all(&dir).unwrap();

        let skill = make_skill(&[dir.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({ "path": "../../etc/passwd" }))
            .await;

        match result {
            Err(SkillError::Forbidden(_)) => {}
            other => panic!("expected Forbidden, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_file_via_symlink_escaping_sandbox_is_forbidden() {
        let sandbox = std::env::temp_dir().join("buddy-test-read-symlink-sandbox");
        let outside = std::env::temp_dir().join("buddy-test-read-symlink-outside");
        std::fs::create_dir_all(&sandbox).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let secret = outside.join("secret.txt");
        std::fs::write(&secret, "secret data").unwrap();

        let link = sandbox.join("escape");
        let _ = std::fs::remove_file(&link);
        symlink(&outside, &link).unwrap();

        let skill = make_skill(&[sandbox.to_str().unwrap()]);
        let result = skill
            .execute(serde_json::json!({ "path": link.join("secret.txt").to_str().unwrap() }))
            .await;

        match result {
            Err(SkillError::Forbidden(_)) => {}
            other => panic!("expected Forbidden, got {other:?}"),
        }

        std::fs::remove_dir_all(&sandbox).ok();
        std::fs::remove_dir_all(&outside).ok();
    }
}
