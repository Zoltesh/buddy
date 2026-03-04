# Formalize Skill Framework

## Description

Currently the codebase conflates Tools and Skills. Skills should be composite operations that use multiple tools with explicit instructions. Create a proper Skill framework.

## Requirements

- Review existing SkillDefinition and Skill structures in skill/mod.rs
- Ensure remember/recall are properly implemented as Skills that use Tools
- Skills should NOT be in the ToolRegistry - they're a higher-level concept
- Create a SkillRegistry separate from ToolRegistry
- Skills support: intent matching via keywords, step-by-step execution

## Current Issues

- remember/recall are registered in ToolRegistry alongside atomic tools
- Skill struct exists but skills aren't really used as intended

## Files to Modify

- `buddy-core/src/skill/mod.rs` - restructure skill framework
- `buddy-core/src/reload.rs` - how skills are loaded

## Dependencies

- Task 009: Formalize Tool Framework

## Acceptance Criteria

- [x] Skills are clearly separated from Tools in the codebase
- [x] Skill framework supports: name, description, tools, instruction_steps, keywords
- [x] Skill matching works (find skill by user intent)
- [x] remember/recall properly implemented as Skills using Tools

## Test Cases

- [x] Run cargo test
- [x] Verify skill matching works for remember/recall
- [x] Verify skills execute with proper instruction steps
