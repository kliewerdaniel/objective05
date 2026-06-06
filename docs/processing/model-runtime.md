# Model Runtime Integration (v1)

## Purpose

Define the concrete integration surface between the extraction pipeline and a local model runtime, and lay out the v1 implementation phases. This document is the bridge between the strategy in `docs/ai/model-strategy.md` and the actual Rust code in `crates/extraction`, `crates/objective`, and a new `crates/model-runtime` crate.

## Scope

- The `ModelRuntime` trait surface that extraction (and eventually correlation, broadcast, contradiction) calls
- The `LocalModelRuntime` implementation that fronts llama.cpp + ONNX
- Configuration, lifecycle, and failure semantics
- The migration path from `HeuristicExtractionService` to a runtime-backed extractor
- Phased delivery so each milestone is shippable behind a feature flag

Out of scope: model download orchestration, training, LoRA adapters, distributed inference, and the plugin-marketplace model surface. Those live in `docs/ai/model-strategy.md` (Future Extensions) and are picked up by later ADRs.

## Assumptions

- The model runtime is a library that ships in the same Cargo workspace as the daemon. There is no separate model-runtime process in v1; the gRPC boundary from `docs/ai/model-strategy.md` is the long-term shape, not the v1 shape (see ADR-013).
- The runtime must degrade to a non-model implementation when no models are downloaded. The v1 default is the existing `HeuristicExtractionService`.
- llama.cpp and ONNX Runtime are native libraries. v1 uses `llama-cpp-rs` and `ort` (ONNX Runtime Rust bindings) and ships behind a `local-models` Cargo feature so CI on machines without the C++ toolchain can still build and test.
- All inference calls are async, are bounded by a configurable timeout, and return a structured `Result` that the caller can fall back from.

## Design

### Trait Surface

```rust
// crates/core/src/traits/runtime.rs
#[async_trait]
pub trait ModelRuntime: Send + Sync {
    /// Identifier (so logs can name the provider)
    fn provider(&self) -> &'static str;

    /// Snapshot of currently-loaded models and their state
    async fn inventory(&self) -> Vec<ModelInfo>;

    /// Run an inference task. Implementations are expected to
    /// honour the per-task timeout; the caller is expected to
    /// still wrap the call in a tokio timeout for hard
    /// upper bounds.
    async fn infer(&self, task: InferenceTask) -> Result<InferenceResult>;

    /// Optional warmup so the first inference does not pay
    /// the load cost. The default implementation is a no-op.
    async fn warmup(&self, _model: ModelId) -> Result<()> { Ok(()) }

    /// Optional shutdown hook for resource cleanup
    async fn shutdown(&self) -> Result<()> { Ok(()) }
}
```

The trait lives in `objective_core::traits` next to `DocumentProcessor` so other consumers (correlation, broadcast, contradiction) can use it without taking a dependency on the extraction crate.

### Task and Result Types

```rust
pub struct InferenceTask {
    pub model: ModelId,                 // e.g. Mistral7BInstruct
    pub kind: InferenceKind,            // NER | ClaimExtraction | Embedding | ...
    pub input: String,                  // already-chunked text
    pub system_prompt: Option<String>,
    pub max_output_tokens: u32,
    pub temperature: f32,               // default 0.0 for extraction
    pub stop: Vec<String>,
    pub timeout: Duration,
}

pub enum InferenceKind {
    NER,
    ClaimExtraction,
    RelationExtraction,
    Embedding,
    TitleGeneration,
    // Future: NarrativeLabeling, ContradictionEvaluation, ReportDrafting
}

pub struct InferenceResult {
    pub text: String,                   // raw LLM output
    pub structured: Option<Value>,      // parsed JSON, if the prompt asked for it
    pub usage: TokenUsage,
    pub model: ModelId,
    pub elapsed: Duration,
}
```

`InferenceKind` keeps the runtime provider-agnostic; the extraction service maps `InferenceKind::ClaimExtraction` to a particular prompt template and a particular model.

### Local Runtime Provider

```rust
// crates/model-runtime/src/local.rs
pub struct LocalModelRuntime {
    llama: RwLock<HashMap<ModelId, Arc<LlamaSession>>>,
    onnx:  RwLock<HashMap<ModelId, Arc<OnnxSession>>>,
    config: LocalRuntimeConfig,
}

impl ModelRuntime for LocalModelRuntime {
    fn provider(&self) -> &'static str { "local-llama-cpp-onnx" }

    async fn inventory(&self) -> Vec<ModelInfo> { ... }

    async fn infer(&self, task: InferenceTask) -> Result<InferenceResult> {
        match classify_kind(&task.kind) {
            KindClass::LLM     => self.run_llama(&task).await,
            KindClass::Embed   => self.run_onnx(&task).await,
            KindClass::Unsupported => Err(ModelError::UnsupportedKind(task.kind)),
        }
    }
}
```

The `LocalRuntimeConfig` is a serialised, file-loaded struct that mirrors the `models` block of the documented `objective.yaml`:

```yaml
models:
  extraction_llm:
    backend: llama_cpp
    path: .objective/models/mistral-7b-instruct-v0.3.Q4_K_M.gguf
    context_tokens: 8192
    gpu_layers: 32
  embedding:
    backend: onnx
    path: .objective/models/bge-small-en-v1.5
    dimension: 384
  default_strategy:
    - kind: ClaimExtraction
      model: extraction_llm
      fallback: heuristic
    - kind: Embedding
      model: embedding
      fallback: none
```

`default_strategy` is the per-task routing table. `fallback: heuristic` is the bridge to v0 — the extraction service knows to retry with `HeuristicExtractionService` when the runtime returns `ModelError::Unavailable` for that kind.

### Integration With `DocumentProcessor`

The v1 extraction service becomes a thin orchestrator on top of the runtime:

```rust
// crates/extraction/src/runtime_extractor.rs
pub struct RuntimeExtractionService {
    runtime: Arc<dyn ModelRuntime>,
    prompt_set: PromptSet,         // the prompt templates per InferenceKind
    fallback: HeuristicExtractionService,
    timeout: Duration,
}

#[async_trait]
impl DocumentProcessor for RuntimeExtractionService {
    async fn process(&self, document: &RawDocument) -> Result<ExtractionResult> {
        let chunks = chunk_document(document, MAX_TOKENS);

        // 1. Embedding (optional)
        let embeddings = self.embed_chunks(&chunks).await.unwrap_or_default();

        // 2. Per-chunk NER + claim + relation extraction
        let mut merged = ExtractionResult::new(&document.id);
        for chunk in &chunks {
            match tokio::time::timeout(self.timeout, self.extract_chunk(chunk)).await {
                Ok(Ok(result)) => merged.merge_from(result),
                Ok(Err(e)) if is_unavailable(&e) => merged.merge_from(self.fallback.process(document).await?),
                Ok(Err(e)) => warn!(?e, "extraction chunk failed"),
                Err(_) => warn!("extraction chunk timed out"),
            }
        }

        // 3. Attach embeddings as a side-channel for the vector store
        merged.metadata.insert("embeddings".into(), json!(embeddings));

        Ok(merged)
    }
}
```

The wiring is a one-liner in `crates/objective/src/app.rs`:

```rust
let runtime: Arc<dyn ModelRuntime> = match config.model_runtime {
    ModelRuntimeConfig::Local { .. } => Arc::new(LocalModelRuntime::from_config(...)?),
    ModelRuntimeConfig::Heuristic => Arc::new(NoopRuntime::heuristic()),
    ModelRuntimeConfig::Disabled  => Arc::new(NoopRuntime::passthrough()),
};
let processor: Arc<dyn DocumentProcessor> = if runtime.is_heuristic() {
    Arc::new(HeuristicExtractionService)
} else {
    Arc::new(RuntimeExtractionService::new(runtime, prompt_set, ...))
};
```

`NoopRuntime` is the v0-compatible provider that never speaks to a model. It exists so the trait surface is always satisfied and the orchestrator code does not have to special-case "no runtime" everywhere.

### Failure Semantics

| Failure | Runtime response | Extraction response |
|---------|------------------|---------------------|
| Model file missing | `ModelError::Unavailable(kind)` | Use heuristic fallback if configured, else skip the chunk and continue |
| Model not loaded yet (lazy load race) | `ModelError::Unavailable(kind)` | Same |
| ONNX/Llama native call returns non-zero | `ModelError::Backend(msg)` | Log, skip chunk, continue with rest of document |
| Caller timeout fires | `tokio::time::Error` | Log, skip chunk, continue |
| Structured parse fails (LLM returned bad JSON) | `ModelResult` with `structured: None` | Parse `text` with the regex fallback parser; if that fails too, emit empty result for that field |

The fallback is **per chunk, per kind**, not per document. A single bad chunk does not fail the whole extraction.

### Configuration Surface

`ObjectiveConfig` gains a `model_runtime: ModelRuntimeConfig` block:

```rust
pub enum ModelRuntimeConfig {
    /// No model loaded; extraction uses HeuristicExtractionService
    Disabled,
    /// Use a heuristic-only shim that always succeeds (for tests + dev)
    Heuristic,
    /// Load llama.cpp + ONNX models from disk
    Local(LocalRuntimeConfig),
}
```

Default: `Disabled` (preserves v0 behaviour). Operators opt in by editing `objective.yaml`.

### Health Surface

`/api/v1/model-runtime` (new endpoint) returns the provider name plus the per-model `ModelState` and last-inference latency. The Recovery Service treats repeated `Backend` errors as a degradation signal and publishes `system.model.degraded` so the dashboard can warn the user.

## Phased Delivery

Each phase is shippable behind a `local-models` Cargo feature so default builds stay small.

### Phase 1 — Trait surface and NoopRuntime (this is the v1 design)
- Land `ModelRuntime` + `InferenceTask` + `InferenceResult` + `ModelError` in `objective-core`
- Land `NoopRuntime::heuristic()` and `NoopRuntime::passthrough()` in `crates/model-runtime`
- Land `RuntimeExtractionService` in `crates/extraction` with the orchestrator code but never hit a real model
- Land config plumbing (`ModelRuntimeConfig::Disabled | Heuristic`) and a feature-flagged wiring in `crates/objective`
- Tests: `RuntimeExtractionService` falls back to heuristic when runtime returns `Unavailable`; default config still produces the v0 outputs

### Phase 2 — ONNX embeddings
- Add `ort` behind the `onnx` feature
- Implement `LocalModelRuntime::run_onnx` for `InferenceKind::Embedding`
- Wire `bge-small-en-v1.5` lookup; document the expected path in `objective.yaml`
- Add a `vector_index: ModelIndex` sidecar on `ExtractionResult` so the existing `LanceDB` stub can consume it

### Phase 3 — llama.cpp LLM
- Add `llama-cpp-rs` behind the `llama` feature
- Implement `LocalModelRuntime::run_llama` for `NER`, `ClaimExtraction`, `RelationExtraction`
- Ship a default prompt set in `crates/extraction/src/prompts/` mirroring `docs/ai/prompting-architecture.md`
- Per-task routing via `default_strategy` block
- Hot model load/unload via a `POST /api/v1/model-runtime/reload` endpoint

### Phase 3.5 — Live swap + real llama.cpp session
- `RuntimeExtractionService` holds `Arc<tokio::sync::RwLock<Arc<dyn ModelRuntime>>>`; the API route and the processor share the handle. `POST /api/v1/model-runtime/reload` takes the write lock, drops in a fresh `LocalModelRuntime`, and the next `process` call picks it up without a daemon restart.
- `crates/model-runtime` gains the `llama` Cargo feature. `llama_cpp = "0.3"` is wired with `default-features = false, features = ["metal", "native"]` so macOS Apple Silicon gets GPU support out of the box. The vendored C++ build compiles llama.cpp from source via the `llama_cpp_sys` build script (cmake + clang + libclang required).
- The real `infer` path runs inside `tokio::task::spawn_blocking` to keep the async runtime responsive. The C++ call sequence is `LlamaModel::load_from_file` → `LlamaModel::create_session` → `LlamaSession::advance_context` → `start_completing_with(StandardSampler::new_greedy(), max_tokens)`. Output is parsed as JSON when possible; if the model produces prose the deterministic stub takes over so the orchestrator still has a well-formed payload to decode.
- The `llama` feature is opt-in. Default builds remain free of native deps.
- Tests: `real_llama_inference_produces_text` runs only when `OBJECTIVE_LLAMA_TEST_MODEL` points at a real GGUF file. `real_llama_missing_model_returns_backend_error` always runs and asserts the orchestrator can fall back per chunk.

### Phase 4 — Lifecycle, observability, and load shedding
- Model state machine from `docs/ai/model-strategy.md` exposed via `/api/v1/model-runtime`
- Per-model inference queue with `tokio::sync::Semaphore` (default 4 concurrent)
- Per-model timeout enforcement at the runtime level (not just the call site)
- Latency histograms exposed via the existing monitoring service

## Testing Strategy

- Unit: `ModelRuntime` mock; `RuntimeExtractionService` tests assert fallback paths and chunk-level error tolerance
- Integration: pipeline tests that swap in a stub runtime returning canned `InferenceResult`; assert that the extraction result composes correctly across chunks
- Snapshot: prompt template regression test — each prompt template is checked in as a fixture and the rendered form is compared
- Performance (Phase 3+): a `bench_model_runtime` harness that loads a small model and reports tokens-per-second

## Cross-References

- `docs/ai/model-strategy.md` — high-level strategy (task-model mapping, resource budget, fallback ladder)
- `docs/ai/prompting-architecture.md` — prompt templates the runtime will execute
- `docs/processing/extraction-engine.md` — pipeline the runtime slots into
- `docs/architecture/architecture-decisions.md` — ADR-004 (local llama.cpp + ONNX), ADR-013 (in-process plugin host, the same precedent applies here), ADR-015 (this design)

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|-----------|
| Trait surface drifts from prompt templates | Runtime calls land on the wrong prompt version | Versioned `PromptSet` keyed by `InferenceKind`; prompt fixture tests |
| llama.cpp / ONNX native panic | Daemon crash | Wrap every native call in `catch_unwind`; mark the model as `Error`; do not retry until reload |
| Embedding model dimension mismatch | LanceDB write fails | Runtime reports the dimension; vector store is built around it at startup |
| Model file on slow disk | First inference latency spike | Optional warmup on `register_builtin`; the runtime exposes a `warmup` hook |
| Heuristic fallback silently masking backend errors | Quality regressions go unnoticed | Runtime emits a `metric: extraction_fallback_total` counter; Recovery Service flips to `Degraded` when the fallback rate exceeds a threshold |

## Future Extensions

- Multi-model ensembles (majority vote) for high-stakes extractions
- LoRA adapter hot-swap for fine-tuned extraction models
- Speculative decoding to lower latency on long context
- The gRPC `Inference` service from `docs/ai/model-strategy.md` (this trait is the in-process version of that contract; the upgrade is mechanical)
- Plugin-provided model backends (a plugin that registers an `Arc<dyn ModelRuntime>` via the existing plugin host)
