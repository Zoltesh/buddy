# 009 — Built-in Skills and Sandbox Configuration

## Description

Implement the three built-in skills (`read_file`, `write_file`, `fetch_url`) and extend the configuration system to support per-skill sandbox settings. Each skill enforces its sandbox constraints — no skill has implicit access to anything.

## Goal

buddy can read files, write files, and fetch URLs — but only within explicitly configured boundaries. The sandbox is the entire security model.

## Requirements

- Extend `buddy.toml` with skill sandbox configuration:
  ```toml
  [skills.read_file]
  allowed_directories = ["/home/user/documents"]

  [skills.write_file]
  allowed_directories = ["/home/user/sandbox"]

  [skills.fetch_url]
  allowed_domains = ["example.com", "api.github.com"]
  ```
- Extend `Config` struct with a `skills` section; all skill config is optional (skills with no config are disabled)
- **`read_file` skill:**
  - Input: `{ "path": "string" }`
  - Output: `{ "content": "string" }`
  - Validates the resolved (canonicalized) path is within an allowed directory
  - Returns `SkillError::Forbidden` if path escapes the sandbox
- **`write_file` skill:**
  - Input: `{ "path": "string", "content": "string" }`
  - Output: `{ "bytes_written": number }`
  - Same path validation as `read_file`
  - Creates parent directories if they don't exist
- **`fetch_url` skill:**
  - Input: `{ "url": "string" }`
  - Output: `{ "status": number, "body": "string" }`
  - HTTP GET only
  - Validates the URL's domain is in the allowlist
  - Returns `SkillError::Forbidden` for non-allowlisted domains
  - Reasonable timeout (10s default)
- Each skill is registered in the `SkillRegistry` at startup (only if configured)
- Update `buddy.example.toml` with commented-out skill configuration examples

## Acceptance Criteria

- [ ] `read_file` reads a file within an allowed directory and returns its content
- [ ] `read_file` rejects paths outside allowed directories (including `../` traversal)
- [ ] `write_file` writes a file within an allowed directory
- [ ] `write_file` rejects paths outside allowed directories
- [ ] `fetch_url` fetches an allowlisted domain and returns status + body
- [ ] `fetch_url` rejects non-allowlisted domains
- [ ] Skills with no configuration in `buddy.toml` are not registered
- [ ] `buddy.example.toml` documents all skill configuration options
- [ ] Existing config parsing (server, provider) is unaffected

## Test Cases

- `read_file` with a path inside an allowed dir: assert returns file content
- `read_file` with `../../etc/passwd`: assert `SkillError::Forbidden`
- `read_file` with a symlink escaping the sandbox: assert `SkillError::Forbidden`
- `write_file` to an allowed dir: assert file is created with correct content
- `write_file` outside allowed dir: assert `SkillError::Forbidden`
- `fetch_url` with an allowlisted domain: assert returns status 200 and non-empty body
- `fetch_url` with a non-allowlisted domain: assert `SkillError::Forbidden`
- Parse a config with no `[skills]` section: assert no skills are registered, server starts normally
- Parse a config with `[skills.read_file]` only: assert only `read_file` is registered
