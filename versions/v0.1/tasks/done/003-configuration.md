# 003 — TOML Configuration

## Description

Implement a configuration system that reads a single TOML file at startup and provides typed access to all runtime settings.

## Goal

All runtime behavior is controlled by a single, documented config file. No environment variables, no CLI flags (beyond an optional `--config` path override).

## Requirements

- A `Config` struct covering:
  - `server.host` (default `"127.0.0.1"`)
  - `server.port` (default `3000`)
  - `provider.api_key` (string, required)
  - `provider.model` (string, required)
  - `provider.endpoint` (URL string, required)
- Parse from `buddy.toml` in the working directory, or a path passed via `--config`
- Fail fast with a clear error message if required fields are missing
- Include an example `buddy.example.toml` with comments explaining every field
- The actual `buddy.toml` is `.gitignore`d (contains secrets)

## Acceptance Criteria

- [x] A valid `buddy.toml` is parsed into a typed `Config` struct at startup
- [x] Missing required fields produce a human-readable error naming the missing field
- [x] `--config /path/to/file.toml` overrides the default location
- [x] `buddy.toml` is in `.gitignore`; `buddy.example.toml` is committed
- [x] Default values for `server.host` and `server.port` work when those fields are omitted

## Test Cases

- Parse a minimal valid config (all required fields); assert struct fields match
- Parse a config missing `provider.api_key`; assert error message contains `"api_key"`
- Parse a config with no `server` section; assert defaults are `127.0.0.1:3000`
- Pass `--config /tmp/test.toml`; assert that file is read instead of `./buddy.toml`
