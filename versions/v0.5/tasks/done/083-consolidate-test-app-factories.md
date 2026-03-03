# 083 — Consolidate Test App Factory Functions

## Description

`buddy-server/src/api/tests.rs` has 4+ factory functions (`test_app()`, `sequenced_app()`, `test_app_with_vector_store()`, `test_app_with_static()`, etc.) that each repeat ~30 lines of `AppState` construction. When `AppState` gains a new field, every factory must be updated — this has been a recurring maintenance burden.

## Goal

A single configurable factory replaces the duplicated app construction, using a builder pattern.

## Requirements

### 1. Create a test app builder

In the test utilities section of `tests.rs` (or `testutil.rs` if test helpers live there), create a builder struct:

```rust
struct TestAppBuilder {
    provider: Option<Arc<dyn ChatProvider>>,
    registry: Option<SkillRegistry>,
    embedder: Option<Arc<dyn Embedder>>,
    vector_store: Option<Arc<dyn VectorStore>>,
    static_dir: Option<String>,
    // ... any other fields the factories currently set
}

impl TestAppBuilder {
    fn new() -> Self { /* defaults */ }
    fn with_provider(mut self, p: Arc<dyn ChatProvider>) -> Self { self.provider = Some(p); self }
    fn with_tokens(self, tokens: Vec<String>) -> Self {
        // Convenience: wraps tokens in a MockProvider
        self.with_provider(Arc::new(MockProvider::new(tokens)))
    }
    fn with_sequenced(self, responses: Vec<MockResponse>, registry: SkillRegistry) -> Self {
        // Convenience: wraps in SequencedProvider + sets registry
        self.with_provider(Arc::new(SequencedProvider::new(responses)))
            .with_registry(registry)
    }
    fn with_registry(mut self, r: SkillRegistry) -> Self { self.registry = Some(r); self }
    fn with_embedder(mut self, e: Arc<dyn Embedder>) -> Self { self.embedder = Some(e); self }
    fn with_vector_store(mut self, v: Arc<dyn VectorStore>) -> Self { self.vector_store = Some(v); self }
    fn with_static_dir(mut self, dir: &str) -> Self { self.static_dir = Some(dir.to_string()); self }
    fn build(self) -> Router { /* construct AppState + Router once */ }
}
```

### 2. Replace existing factories

Replace each existing factory's body with the builder:

- `test_app(tokens)` → `TestAppBuilder::new().with_tokens(tokens).build()`
- `sequenced_app(responses, registry)` → `TestAppBuilder::new().with_sequenced(responses, registry).build()`
- `test_app_with_vector_store(e, v)` → `TestAppBuilder::new().with_embedder(e).with_vector_store(v).build()`
- `test_app_with_static(tokens, dir)` → `TestAppBuilder::new().with_tokens(tokens).with_static_dir(dir).build()`

You may keep the old function signatures as thin wrappers for convenience, or inline the builder calls at each call site — whichever results in less code.

### 3. Single source of truth for AppState construction

The `build()` method is the ONE place that constructs `AppState`. When a new field is added to `AppState` in the future, only `build()` needs updating.

## Files to Modify

- `buddy-server/src/api/tests.rs` — add builder, refactor factory functions
- Possibly `buddy-server/src/testutil.rs` — if the builder belongs there

## Acceptance Criteria

- [ ] `AppState` is constructed in exactly one place (the builder's `build()` method)
- [ ] All existing tests still pass (`cargo test`)
- [ ] No duplicate `AppState` construction logic remains
- [ ] Adding a new field to `AppState` requires changing only the builder

## Test Cases

- [ ] Run `cargo test`; assert all ~400+ tests pass (pure refactor, zero behavior change)
- [ ] Verify the builder compiles with each combination: tokens only, sequenced, with embedder, with static dir
