# Model Strategy

## Purpose

Define Objective's model strategy — which models are used for which tasks, how they are managed, the runtime architecture, and resource requirements.

## Scope

This document covers supported model types, task-to-model routing, model lifecycle management, context window management, hardware requirements, and fallback strategies.

## Responsibilities

- Define model selection criteria per task
- Specify model runtime architecture (llama.cpp + ONNX)
- Document model lifecycle (download, load, update, unload)
- Define task routing and fallback logic
- Specify resource requirements and constraints
- Document context window management strategies

## Assumptions

- All models run locally (see ADR-001, ADR-004)
- Model quality improves over time; upgrade path is essential
- Consumer hardware is the target (32GB RAM, 8GB VRAM minimum)
- Models may be unavailable during updates or if hardware is insufficient
- The system degrades gracefully when models are unavailable

## Design

### Task-Model Mapping

| Task | Model Class | Recommended Model | Quality Level | Runtime |
|------|-------------|-------------------|---------------|---------|
| Entity Extraction (NER) | Embedding + Classifier | bge-small-en-v1.5 + linear head | Fast, good | ONNX |
| Claim Extraction | Small LLM | Mistral 7B Instruct | Good | llama.cpp |
| Relationship Extraction | Small LLM | Mistral 7B Instruct | Good | llama.cpp |
| Event Indicator Extraction | Small LLM | Mistral 7B Instruct | Good | llama.cpp |
| Text Embedding | Embedding | bge-small-en-v1.5 (384d) | Fast, good | ONNX |
| Document Similarity | Embedding | bge-small-en-v1.5 | Fast | ONNX |
| Narrative Labeling | Medium LLM | Mixtral 8x7B Instruct | Very good | llama.cpp |
| Contradiction Detection | Small LLM | Mistral 7B Instruct | Good | llama.cpp |
| Report Generation | Medium LLM | Mixtral 8x7B Instruct | Best | llama.cpp |
| Title Generation | Small LLM | Mistral 7B Instruct | Fast, good | llama.cpp |
| Summary Generation | Medium LLM | Mixtral 8x7B Instruct | Very good | llama.cpp |
| Audio Transcription | ASR | whisper.cpp (base.en) | Good | whisper.cpp |
| Text Classification | Embedding + Classifier | bge-small-en-v1.5 + linear head | Fast | ONNX |

### Model Tier Classification

| Tier | Size | VRAM | RAM | Performance | Use Case |
|------|------|------|-----|-------------|----------|
| Small | < 4GB | 0 GB (CPU) | 4GB | Fast | Entity extraction, embeddings, classification |
| Medium | 4-8GB | 4-6GB | 8GB | Moderate | Claim extraction, contradiction detection |
| Large | 8-16GB | 6-8GB | 16GB | Slower | Report generation, narrative labeling |
| ASR | < 2GB | 0-2GB | 2GB | Fast | Audio transcription |

### Model Runtime Architecture

```
┌──────────────────────────────────────────────┐
│               Model Runtime Service            │
│                                                │
│  ┌─────────────┐  ┌─────────────┐              │
│  │ llama.cpp    │  │ ONNX Runtime│              │
│  │ (LLM tasks)  │  │ (embedding) │              │
│  │              │  │             │              │
│  │ Model Pool:  │  │ Model Pool: │              │
│  │ - Mistral 7B │  │ - bge-small │              │
│  │ - Mixtral    │  │             │              │
│  └──────┬───────┘  └──────┬──────┘              │
│         │                 │                     │
│         └─────────────────┘                     │
│                        │                        │
│  ┌─────────────────────┴──────────────────────┐ │
│  │           Inference Queue                    │ │
│  │  (Prioritized, per-model, bounded: 10 max)  │ │
│  └─────────────────────┬──────────────────────┘ │
│                        │                        │
│  ┌─────────────────────┴──────────────────────┐ │
│  │         Inference API (gRPC)                │ │
│  └────────────────────────────────────────────┘ │
└────────────────────────────────────────────────┘
```

**Key design decisions:**

1. **Model pooling:** Multiple models loaded simultaneously; models are shared across all services
2. **Dedicated queue per model:** Prevents a burst of extraction tasks from starving broadcast generation
3. **Bounded queue:** Max 10 pending requests per model; overflow returns 503
4. **GPU priority:** Large models get GPU priority; small models run on CPU
5. **Graceful degradation:** If large model is busy, fall back to small model for appropriate tasks
6. **Hot-swapping:** Models can be loaded/unloaded without restarting the runtime

### Context Window Management

LLM tasks have varying context requirements:

| Task | Context Window | Strategy |
|------|---------------|----------|
| Entity extraction | 4096 tokens | Process document chunks |
| Claim extraction | 4096 tokens | Process each chunk independently |
| Narrative labeling | 8192 tokens | Batch top-20 events |
| Report generation | 32768 tokens | Multi-step: facts → outline → draft → polish |
| Contradiction evaluation | 4096 tokens | Include both claims + entity context |

**Context overflow strategies:**

```rust
pub fn prepare_context(task: &Task, input: &str, max_tokens: usize) -> String {
    let token_count = estimate_tokens(input);

    if token_count <= max_tokens {
        return input.to_string();
    }

    match task.strategy {
        // Truncate from middle: keep head (instructions) and tail (content)
        ContextStrategy::HeadTail => {
            let head = head(input, max_tokens / 2);
            let tail = tail(input, max_tokens / 2 - 100);
            format!("{}\n[...truncated {} tokens...]\n{}", head, token_count - max_tokens, tail)
        }
        // Summarize input first, then process summary
        ContextStrategy::SummarizeThenProcess => {
            let summary = summarize(input, max_tokens / 2);
            prepare_context(task, &summary, max_tokens / 2)
        }
        // Split into chunks, process each, merge results
        ContextStrategy::Chunked => {
            // Returns full input; caller's responsibility to chunk
            input.to_string()
        }
    }
}
```

### Model Lifecycle

```rust
pub enum ModelState {
    NotDownloaded,
    Downloading { progress: f32 },
    Downloaded,
    Loading,
    Ready,
    Busy,
    Error { message: String },
    Unloading,
}
```

**Lifecycle:**

1. **Discovery:** At startup, scan model directory for available models
2. **Download:** If model not present, download from HuggingFace or mirror
3. **Validation:** Verify model file checksum (SHA-256)
4. **Loading:** Load model into memory (llama.cpp: mmap, ONNX: session creation)
5. **Ready:** Model available for inference
6. **Inference:** Process requests through queue
7. **Unloading:** Unload model to free memory (on user request or memory pressure)
8. **Update:** Periodically check for newer model versions

**Download:**
```yaml
models:
  mistral-7b:
    source: "https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.3-GGUF"
    file: "mistral-7b-instruct-v0.3.Q4_K_M.gguf"
    checksum: "sha256:abc123..."
    size_gb: 4.1
```

### Task Routing

```rust
pub struct TaskRoute {
    pub model: ModelId,           // Which model to use
    pub priority: TaskPriority,   // Low, Normal, High
    pub timeout: Duration,        // Max inference time
    pub fallback: Option<ModelId>, // Fallback model if primary unavailable
}

pub fn route_task(task: &InferenceTask) -> TaskRoute {
    match task.task_type {
        TaskType::EntityExtraction => TaskRoute {
            model: ModelId::Mistral7B,
            priority: TaskPriority::Normal,
            timeout: Duration::from_secs(30),
            fallback: Some(ModelId::Mistral7B_Quantized), // Q3_K_M if Q4 available
        },
        TaskType::ReportGeneration => TaskRoute {
            model: ModelId::Mixtral8x7B,
            priority: TaskPriority::Low,    // Reports are not time-critical
            timeout: Duration::from_secs(120),
            fallback: Some(ModelId::Mistral7B), // Reduced quality but functional
        },
        TaskType::Embedding => TaskRoute {
            model: ModelId::BGESmall,
            priority: TaskPriority::Normal,
            timeout: Duration::from_secs(5),
            fallback: None,                   // No embedding = system degraded
        },
        // ...
    }
}
```

### Resource Requirements

**Minimum hardware (degraded mode):**
- CPU: 4 cores, x86_64 (AVX2 support recommended)
- RAM: 16 GB
- Storage: 20 GB free
- GPU: None (CPU-only inference)

**Recommended hardware:**
- CPU: 8 cores
- RAM: 32 GB
- GPU: 8 GB VRAM (NVIDIA, AMD, or Apple Silicon)
- Storage: 100 GB SSD
- Network: Broadband (for model downloads and source ingestion)

**Apple Silicon:**
- Uses Metal backend for GPU acceleration
- Unified memory reduces VRAM/RAM split concerns
- Minimum: M1 with 16GB unified memory
- Recommended: M2/M3/M4 with 24GB+ unified memory

**Memory budget:**
```yaml
memory_budget:
  total: "32GB"
  system_reserve: "4GB"        # OS and other applications
  graph_db: "4GB"              # Kuzu buffer pool
  vector_db: "1GB"             # LanceDB
  models:
    mistral_7b: "4GB"          # Q4_K_M quantization
    mixtral_8x7b: "8GB"        # Q4_K_M quantization
    bge_small: "512MB"
    whisper: "1GB"
  processing_buffer: "2GB"     # Document processing, extraction results
  queue_storage: "1GB"         # NATS JetStream
  remaining: "6.5GB"           # Headroom for peaks
```

### Fallback Strategy

If a model is unavailable (not downloaded, OOM, crash):

| Primary | Fallback | Quality Impact | Tasks Affected |
|---------|----------|---------------|----------------|
| Mixtral 8x7B | Mistral 7B | Reduced quality | Report gen, narrative labeling |
| Mistral 7B | None | Cannot process | Extraction, contradiction |
| bge-small | BM25 (sparse) | Worse recall | Similarity search |
| whisper | No transcription | No podcast text | Audio sources only |
| Any LLM | No extraction | System degrades | All processing stops |

**Model readiness check:**
```rust
pub fn check_model_readiness() -> SystemReadiness {
    let has_llm = is_model_ready(ModelId::Mistral7B)
        || is_model_ready(ModelId::Mixtral8x7B);
    let has_embeddings = is_model_ready(ModelId::BGESmall);
    let has_asr = is_model_ready(ModelId::Whisper);

    match (has_llm, has_embeddings) {
        (true, true) => SystemReadiness::Full,
        (true, false) => SystemReadiness::Degraded("No embedding model"),
        (false, _) => SystemReadiness::Degraded("No LLM available - extraction paused"),
    }
}
```

### Structured Output Parsing

All LLM tasks use structured output parsing:

```rust
pub struct StructuredOutputConfig {
    pub format: OutputFormat,     // JSON, YAML, Markdown
    pub schema: String,           // JSON Schema for validation
    pub max_retries: u32,         // 3 - retry on malformed output
    pub fallback_parser: Option<FallbackParser>, // Regex-based parser
}
```

**Parsing strategy:**
1. Request structured output in prompt (JSON with schema)
2. Attempt JSON parse
3. If parse fails: retry with stricter prompt (max 3)
4. If all retries fail: use fallback parser (regex extraction)
5. If fallback fails: return empty result for this chunk
6. Log parsing failures for prompt improvement

## Interfaces

- `prompting-architecture.md` — how prompts interact with models
- `docs/architecture/architecture-decisions.md` — ADR-004 (model runtime)
- `docs/processing/extraction-engine.md` — uses models for extraction
- `docs/processing/contradiction-engine.md` — uses models for evaluation

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| OOM during inference | Service crash | Pre-allocate model memory; monitor memory; graceful degradation |
| Model download fails | Model unavailable | Resume download on retry; checksum verification |
| Slow inference degrades pipeline | Extraction/processing backlog | Queue with backpressure; timeout per task |
| GPU not available (expected but missing) | CPU-only inference (slower) | Check GPU at startup; adjust expectations |
| Model file corrupted | Load failure | SHA-256 verification; auto re-download |
| Quantization artifacts | Reduced quality | Accept for speed; document quality tradeoffs |
| Hot-swap during inference | Request failure | Drain queue before swap; retry on consumer side |

## Future Extensions

- Multi-model ensembles for extraction (majority vote)
- LoRA adapter support for fine-tuned extraction models
- Speculative decoding for faster inference
- Dynamic model quantization selection based on available memory
- Model marketplace in plugin ecosystem
- Fine-tuning pipeline for domain-specific extraction
- Distributed inference across multiple machines
