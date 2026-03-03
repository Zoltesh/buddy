# Create Skill Concept for Composite Operations

## Description

After renaming `Skill` to `Tool`, we need a new `Skill` concept for composite, high-level operations that use tools to accomplish specific purposes. Skills include explicit instructions, validation steps, and decision logic.

## Goal

Create a new `Skill` struct/format that represents composite operations, distinct from atomic tools.

## Requirements

- Create a new `Skill` struct in `buddy-core/src/skill/`
- Define a structured format for skill definitions (YAML/JSON compatible)
- Skills should reference tools by name
- Include instruction steps, validation logic, and user prompts
- Skills are loaded from a registry (similar to tools)
- Implement skill matching: given user input, determine if a skill applies

## Files to Modify

- `buddy-core/src/skill/mod.rs` — add new Skill struct/format
- May need new files for skill loading/parsing

## Acceptance Criteria

- [x] New `Skill` struct exists and is distinct from `Tool`
- [x] Skill definitions include: name, description, tools used, instruction steps
- [x] Skills can be matched to user intent
- [x] Skills can use tools to accomplish goals

## Test Cases

- [x] Create a sample skill definition (e.g., "create_document")
- [x] Verify skill can be parsed from structured format
- [x] Verify skill can call underlying tools
