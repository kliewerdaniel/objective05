# Coding Standards

## Purpose

Define the coding standards for Objective development, ensuring consistency, maintainability, and quality across all code in the repository.

## Scope

This document covers naming conventions, code style, testing requirements, documentation standards, type safety requirements, logging standards, and code review practices.

## Responsibilities

- Define coding conventions for all languages in use
- Establish testing requirements and coverage targets
- Specify type safety requirements
- Define logging and error handling standards
- Document code review expectations
- Define automated enforcement (linters, formatters)

## Assumptions

- Rust is the primary language; TypeScript is the secondary language for dashboard
- Contributors may be external; standards should be documented and enforced automatically
- CI enforces standards; code review catches what automation misses
- Standards evolve; changes are documented in this file

## Design

### Language-Specific Standards

#### Rust Standards

**Style:**
- Use `rustfmt` with default settings (enforced in CI)
- Use `clippy` with no warnings (enforced in CI)
- Follow Rust API Guidelines (RFC 968)
- Maximum line length: 120 characters

**Naming conventions:**
| Element | Convention | Example |
|---------|-----------|---------|
| Types | PascalCase | `ExtractedEntity` |
| Traits | PascalCase | `SourceAdapter` |
| Functions | snake_case | `extract_entities()` |
| Methods | snake_case | `document.process()` |
| Variables | snake_case | `entity_count` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_RETRY_COUNT` |
| Modules | snake_case | `entity_extractor` |
| Files | snake_case.rs | `entity_extractor.rs` |
| Private | no prefix | `fn process_internal()` |
| Public | no prefix | `pub fn process()` |
| Type parameters | short PascalCase | `T`, `E: Error` |

**Module structure:**
```rust
// Prefer:
// src/lib.rs — Public API re-exports
// src/module_name.rs — Module implementation
// src/module_name/ — Sub-modules (when > 500 lines)

// lib.rs
pub mod extractor;
pub mod merger;

// extractor.rs (single file for < 500 lines)
pub fn extract_entities() {}

// merger/ (directory for > 500 lines)
// merger/mod.rs
// merger/dedup.rs
// merger/merge_strategy.rs
```

**Error handling:**
```rust
// Use thiserror for library errors
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    #[error("chunk too large: {size} tokens")]
    ChunkTooLarge { size: usize },

    #[error("model inference failed: {source}")]
    ModelError {
        #[from]
        source: ModelRuntimeError,
    },

    #[error("parse failed: {0}")]
    ParseError(String),
}

// Use anyhow for application errors (main binary)
pub fn run_pipeline() -> anyhow::Result<()> {
    let result = extract_entities(&document)
        .context("Failed to extract entities from document")?;
    Ok(())
}
```

**Concurrency:**
- Use `tokio` for async runtime
- Use `rayon` for CPU-bound parallel processing
- Prefer `async/await` over manual futures
- Use `tokio::sync` channels for message passing
- Use `Arc<Mutex<T>>` only when necessary; prefer `tokio::sync::RwLock`
- Mark `Send + Sync` on shared types
- Document unsafe code with `// SAFETY:` comments

**Documentation:**
```rust
/// Extract named entities from a text chunk.
///
/// Uses the configured LLM to identify and extract entities.
///
/// # Arguments
/// * `chunk` - The text chunk to analyze (max 4096 tokens)
/// * `model` - The LLM model to use for extraction
///
/// # Returns
/// A list of extracted entities with confidence scores
///
/// # Errors
/// Returns `ExtractionError::ChunkTooLarge` if chunk exceeds model limit
/// Returns `ExtractionError::ModelError` if inference fails
///
/// # Example
/// ```rust
/// let entities = extract_entities(&chunk, &model).await?;
/// ```
pub async fn extract_entities(chunk: &str, model: &Model) -> Result<Vec<Entity>>;
```

**Testing:**
- Unit tests in same file: `#[cfg(test)] mod tests { ... }`
- Integration tests in `tests/` directory
- Test function names: `fn test_<description>()`
- Use `assert_eq!`, `assert!`, `anyhow::Result` in tests
- Mock external dependencies with traits + test implementations
- Property-based testing for parsers (using `proptest` or `quickcheck`)

#### TypeScript / React Standards

**Style:**
- Use `prettier` with default settings (enforced in CI)
- Use `eslint` with typescript-eslint recommended rules
- Maximum line length: 100 characters

**Naming conventions:**
| Element | Convention | Example |
|---------|-----------|---------|
| Components | PascalCase | `EventCard` |
| Hooks | camelCase, prefixed with "use" | `useEventData` |
| Functions | camelCase | `formatDate()` |
| Variables | camelCase | `entityCount` |
| Types/Interfaces | PascalCase | `EventData` |
| Files (components) | PascalCase | `EventCard.tsx` |
| Files (utilities) | camelCase | `formatDate.ts` |
| CSS classes | kebab-case | `event-card-title` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_VISIBLE_ITEMS` |

**Component structure:**
```typescript
// Prefer function components with hooks
const EventCard: React.FC<EventCardProps> = ({ event, onSelect }) => {
    const { data, isLoading } = useEventData(event.id);

    if (isLoading) return <LoadingSpinner />;
    return <div>{/* ... */}</div>;
};
```

**State management:**
- Zustand for global state (pre-defined stores)
- React Query (TanStack Query) for server state
- Local state with `useState` for component-specific state
- Avoid prop drilling; use store or context

**Testing:**
- Vitest for unit tests
- React Testing Library for component tests
- Playwright for E2E tests
- Test file co-located: `EventCard.test.tsx`

### All Languages

**Logging:**
```rust
// Rust: use the `tracing` crate
use tracing::{info, warn, error, debug, trace, instrument};

#[instrument(skip(document))]
pub async fn process_document(document: &Document) -> Result<()> {
    info!("Processing document");
    debug!(size = document.size(), "Document size check");
    // ...
}
```

```typescript
// TypeScript: use a structured logger
import { logger } from '@/lib/logger';

logger.info('Processing document', { documentId: doc.id, size: doc.size });
```

Log levels:
- `ERROR`: Service failure, data loss, crashes
- `WARN`: Degraded performance, retry exhaustion
- `INFO`: Normal operations (polls, extractions, broadcasts)
- `DEBUG`: Detailed operational info (off by default)
- `TRACE`: Per-item tracing (off by default, high volume)

**Error handling (all languages):**
- Never panic/unwrap in production code
- Handle all errors explicitly
- Return structured errors, not strings
- Include context in errors ("Failed to X: reason")
- Log errors at the appropriate level
- Errors in callbacks should be logged, not ignored

**Comments:**
- Prefer self-documenting code over comments
- Use comments to explain WHY, not WHAT
- TODO comments must include issue reference: `// TODO(#123): fix this`
- No commented-out code (use version control)
- Document public API surfaces (Rustdoc, JSDoc)

**Security (all languages):**
- No secrets in code (use environment variables or config files)
- Validate all external input
- Sanitize all output (prevent injection)
- Use parameterized queries for all database operations
- No `eval()` or dynamic code execution
- No unsafe deserialization of untrusted data

### Testing Requirements

**Coverage targets:**
| Area | Target | Minimum |
|------|--------|---------|
| Core types and traits | 95% | 90% |
| Service business logic | 90% | 80% |
| Graph queries | 90% | 80% |
| Source adapters | 80% | 70% |
| API endpoints | 85% | 75% |
| Dashboard components | 80% | 70% |

**Test types required:**
1. **Unit tests**: Every function with non-trivial logic
2. **Integration tests**: Every service boundary (message bus communication)
3. **Contract tests**: API endpoint responses match schemas
4. **Performance tests**: Critical paths have latency budgets
5. **Property-based tests**: Parsers, validators, serializers

**Test naming convention:**
```rust
// Rust
#[test]
fn test_extract_entities_returns_entities_for_valid_text() {}

#[test]
fn test_extract_entities_returns_empty_for_no_entities() {}

#[test]
fn test_extract_entities_errors_on_empty_input() {}
```

**Test fixtures:**
- Shared test data in `tests/common/fixtures/`
- Factory functions for creating test entities
- Snapshot testing for complex output structures
- Use builder pattern for test data:

```rust
Entity::builder()
    .name("Apple Inc")
    .entity_type(EntityType::Organization)
    .confidence(0.95)
    .build()
```

### Code Review Standards

**Review requirements:**
- All code changes require review (no direct pushes to main)
- At least one approval from a domain expert
- CI must pass before merge
- No PRs over 500 lines of code changes (split into smaller PRs)

**Review checklist:**
- [ ] Code follows coding standards (checkstyle/lint)
- [ ] Tests included and passing
- [ ] Error handling is appropriate
- [ ] Logging at appropriate levels
- [ ] No security issues (injection, secrets, unsafe)
- [ ] Documentation updated if API changed
- [ ] Performance considerations addressed
- [ ] Backward compatibility maintained
- [ ] No unnecessary dependencies added

### Automated Enforcement

**CI pipeline stages:**
1. `lint` — rustfmt, clippy, eslint, prettier
2. `typecheck` — cargo check, tsc
3. `test` — cargo test, vitest
4. `integration-test` — cargo test --test
5. `coverage` — coverage report (verified)
6. `build` — cargo build --release, vite build
7. `security-scan` — dependency audit (cargo audit, npm audit)

**Pre-commit hooks:**
- Formatting (rustfmt, prettier)
- Linting (clippy on changed files, eslint)
- No secrets detection (git-secrets)
- No large files (> 1MB)

### Dependency Management

- All dependencies pinned with lockfiles
- No "latest" version constraints
- Weekly dependency audit
- Dependency review for new dependencies:
  - Is it actively maintained?
  - Is the license compatible (MIT, Apache 2.0, BSD)?
  - Is it a significant size addition?
  - Could the functionality be implemented in-house with less overhead?
- Prefer Rust crates over system libraries (portability)
- Minimize transitive dependencies

### Documentation in Code

- All public API surfaces have doc comments
- Architecture decisions documented in ADRs
- Complex algorithms documented with references
- Configuration files have inline comments
- README for each major crate/service

## Interfaces

- `repository-structure.md` — where code goes
- `docs/architecture/architecture-decisions.md` — decisions that constrain coding choices

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Standards not followed | Inconsistent codebase | Enforce via CI; code review catches remaining |
| Standards outdated | Practices ossify | Annual review; propose changes via PR |
| Coverage targets not met | Untested code | CI blocks below threshold; track coverage trends |
| Overly strict standards | Slow development | Balance with pragmatism; exceptions documented |
| Review bottleneck | Delayed merges | Rotating reviewers; clear review expectations |
| Dependency bloat | Compilation time, security surface | Dependency review process; monthly audit |

## Future Extensions

- Automated documentation generation from doc comments
- Performance regression detection in CI
- Fuzz testing for parsers
- Mutation testing for test quality
- Standardized benchmarking framework
- Accessibility testing automation for dashboard
