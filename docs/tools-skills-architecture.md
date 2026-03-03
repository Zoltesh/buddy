# Tools vs Skills Architecture

## Overview

This document defines the architectural separation between **Tools** and **Skills** in Buddy, and outlines the implementation plan.

## Definitions

### Tools

Atomic, low-level capabilities that Buddy can invoke directly. Tools are:
- **Independent** - Each tool does one specific thing
- **Discoverable** - Exposed to the LLM with schema and descriptions
- **Grounded** - No instruction logic, just parameters + execution

Examples:
- `read_file` - Read a file from an allowed directory
- `write_file` - Write content to a file
- `fetch_url` - HTTP GET request to allowed domains
- `memory_read` / `memory_write` - Working memory operations

### Skills

Composite, high-level instructions that accomplish a purpose. Skills are:
- **Intent-driven** - Match user intent to a defined workflow
- **Instructive** - Include explicit steps, validation, error handling
- **Composable** - Use tools (and potentially other skills) to achieve goals

Examples:
- `remember` - Stores facts in long-term vector memory with category
- `recall` - Searches long-term memory with semantic similarity
- A hypothetical `create_document` skill would include: asking for filename/directory, checking if file exists, writing, validating, confirming

## Decision Engine

When a user makes a request, Buddy should:

```
1. Check skills for intent match
   → If skill matches → Follow skill instructions

2. If no skill matches → Direct tool call
   → If request is simple/ambiguous enough → Use tool directly
```

**Key principle:** Buddy is smart enough to know when a direct tool call is sufficient vs. when a skill's explicit instructions are needed.

## Current State

Currently, everything is implemented as a `Skill` in `buddy-core/src/skill/`. This conflates tools and skills.

## Implementation Phases

### Phase 1: Core Tools/Skills Separation (Next)

**Goal:** Establish clear delineation, update UI

**Changes:**
1. Rename `Skill` trait → `Tool` for atomic capabilities
2. Create new `Skill` concept for composite operations
3. Keep existing implementations as tools
4. Create explicit skills where they add value
5. Split Settings UI: separate "Tools" tab from "Skills" tab

**UI Changes:**
- **Tools tab** (new): Lists all available tools with descriptions
- **Skills tab**: Lists skills with descriptions and which tools they use
- Both tabs are read-only for system tools/skills (at this phase)

### Phase 2: Skill Creation via Chat

**Goal:** Users can ask Buddy to create skills

**Behavior:**
1. User asks "Create a skill for X"
2. Buddy's "skill creator" skill activates
3. Buddy analyzes request, checks available tools
4. If tools exist → Creates skill definition (YAML/JSON)
5. If tools missing → Tells user "I don't have the tools to do this yet"

**Scope:** Skill creation only - no new tool creation yet

### Phase 3: Tool Creation Skill

**Goal:** Buddy can create new tools

**Behavior:**
1. User asks for capability Buddy doesn't have
2. Buddy determines tool is needed
3. Buddy generates code (starting with Python, maybe Rust later)
4. Buddy validates and tests the code
5. Tool is registered and available

**Constraints:** Limited code generation capability initially

### Phase 4: Autonomous Skill/Tool Creation

**Goal:** Buddy is fully autonomous in creating skills and tools

**Behavior:**
1. Buddy researches (web fetch) for best approaches
2. Considers multiple languages/approaches
3. Generates, tests, fixes code
4. Creates skill definition with detailed instructions
5. Validates end-to-end

**Scope:** Python first, potentially Rust/JavaScript

## User Space (Future)

Eventually, user-created skills and tools live separately from Buddy's core:

```
~/.buddy/
  tools/          # User-created tools (Python, Rust, etc.)
  skills/        # User-created skill definitions
  data/          # User data and memories
```

## Skill Definition Format

Skills should be structured (YAML or JSON) so Buddy can parse and reason about them:

```yaml
name: create_text_file
description: Create and validate a text document
triggers:
  - "create a document"
  - "write a file"
tools:
  - write_file
  - read_file
instructions:
  - step: 1
    action: ask_user
    prompt: "What should the filename be?"
    required: true
  - step: 2
    action: ask_user
    prompt: "What directory? (default: ~/buddy/files)"
    default: "~/buddy/files"
  - step: 3
    action: check_exists
    tool: read_file
    on_exists: ask_overwrite
  - step: 4
    action: call_tool
    tool: write_file
  - step: 5
    action: validate
    tool: read_file
    compare_input: true
  - step: 6
    action: confirm
    message: "File created successfully"
```

## Open Questions

1. **Where do user skills live?** Project-level? Home directory? Both?
2. **How does Buddy discover user skills?** Scan on startup? Hot reload?
3. **Tool execution sandbox?** If Python tools are allowed, how to run safely?
4. **Skill format:** YAML or JSON? Markdown for humans + parsed format?

## Related Tasks

- Phase 1 tasks needed:
  - Refactor `Skill` trait → `Tool`
  - Create `Skill` struct/format for composite operations
  - Split Settings UI into Tools/Skills tabs
  - Identify which current implementations are tools vs. skills
