# 002 — Core Message Types

## Description

Define the foundational message types that represent all possible entries in a conversation. These types are used across the entire system — provider, API, and frontend — so they must be designed for forward compatibility with skills and tool calls even though V0.1 does not implement them.

## Goal

A single canonical set of types that every layer of the system serializes and deserializes without transformation.

## Requirements

- A `Role` enum: `User`, `Assistant`, `System`
- A `Message` struct containing: `role`, `content` (text), `timestamp`
- A `MessageContent` enum that can represent: `Text(String)`, `ToolCall { id, name, arguments }`, `ToolResult { id, content }`
- All types derive `Serialize`, `Deserialize`, `Clone`, `Debug`
- All types are defined in a `types` module at the crate root
- `ToolCall` and `ToolResult` variants exist in the enum but are not used in V0.1 — they are there to lock in the shape

## Acceptance Criteria

- [ ] Types compile and round-trip through `serde_json` without data loss
- [ ] A `Vec<Message>` can represent a multi-turn conversation with mixed roles
- [ ] `ToolCall` and `ToolResult` variants serialize/deserialize correctly even though nothing produces them yet

## Test Cases

- Serialize a `Vec<Message>` with User, Assistant, and System messages to JSON; deserialize it back; assert equality
- Serialize a `Message` with `ToolCall` content; deserialize; assert fields match
- Serialize a `Message` with `ToolResult` content; deserialize; assert fields match
