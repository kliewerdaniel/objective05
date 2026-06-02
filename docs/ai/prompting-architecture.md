# Prompting Architecture

## Purpose

Define Objective's prompting architecture — how prompts are managed, versioned, and executed across all LLM tasks, including structured output contracts and reliability requirements.

## Scope

This document covers prompt lifecycle, prompt versioning, structured output enforcement, prompt templates, context construction, retry strategies, and quality monitoring.

## Responsibilities

- Define prompt management lifecycle
- Specify prompt versioning and storage
- Define structured output contracts
- Document reliability and retry strategies
- Define prompt quality monitoring

## Assumptions

- Prompts are templates with variable interpolation, not static strings
- Prompt quality directly impacts system quality
- Prompts need versioning to track improvements and regressions
- Different tasks require different prompt structures
- The system must gracefully handle prompt failures (malformed output, empty results)

## Design

### Prompt Lifecycle

```
Draft → Test → Review → Versioned → Deployed → Monitored → Iterated
                           │                           │
                           └── Archived (if superseded) └── Rollback (if regressed)
```

**Stages:**
1. **Draft:** Author prompt template with expected output schema
2. **Test:** Run against test corpus (pre-defined documents with known expected extractions)
3. **Review:** Compare output quality against current production prompt
4. **Versioned:** If quality improves, commit as new version
5. **Deployed:** All new inference requests use this version
6. **Monitored:** Track quality metrics (parse success rate, empty result rate, user corrections)
7. **Iterated:** If quality degrades or new capabilities needed, start new draft
8. **Archived:** Old versions retained for rollback and comparison

### Prompt Registry

Prompts are stored as versioned YAML files:

```
prompts/
├── v1/
│   ├── entity-extraction.yaml
│   ├── claim-extraction.yaml
│   ├── relationship-extraction.yaml
│   ├── narrative-labeling.yaml
│   ├── contradiction-evaluation.yaml
│   ├── report-generation.yaml
│   ├── title-generation.yaml
│   └── event-description.yaml
├── v2/
│   └── ...
├── current -> v2               # Symlink to active version
└── manifest.json               # Version metadata
```

**Prompt template structure:**

```yaml
# prompts/v2/entity-extraction.yaml
name: entity-extraction
version: 2
task: entity_extraction
model: mistral-7b-instruct
description: Extract named entities from text

# Expected output schema
output_schema:
  type: array
  items:
    type: object
    properties:
      name:
        type: string
        description: Canonical entity name
      entity_type:
        type: string
        enum: [Person, Organization, Location, Concept, Event_Topic]
      confidence:
        type: number
        minimum: 0
        maximum: 1
      aliases:
        type: array
        items:
          type: string

# System prompt (pre-pended to conversation)
system: |
  You are an entity extraction system. Extract all named entities from the provided text.
  Focus on specific, named entities. Do not extract generic concepts unless they are named.

# User prompt template (with {variables})
user: |
  Extract all named entities from the following text.

  Text:
  {chunk}

  Return a JSON array of entities:
  [
    {
      "name": "Entity Name",
      "entity_type": "Person|Organization|Location|Concept|Event_Topic",
      "confidence": 0.95,
      "aliases": ["Alternate Name"]
    }
  ]

  If no entities are found, return an empty array [].

# Validation rules
validation:
  require_nonempty: false       # Empty results are valid
  min_items: 0
  max_items: 50
  validate_types: true          # entity_type must be in enum
  confidence_range: [0.0, 1.0]

# Test cases
tests:
  - input: "Apple CEO Tim Cook announced..."
    expected:
      - name: "Tim Cook"
        entity_type: "Person"
      - name: "Apple"
        entity_type: "Organization"
  - input: "The weather is nice today."
    expected: []

# Performance targets
performance:
  max_tokens: 4096
  expected_duration_ms: 5000
  max_retries: 2
  fallback_strategy: "return_empty"
```

### Prompt Template Variables

Standard variables available in prompt templates:

| Variable | Description | Source |
|----------|-------------|--------|
| `{chunk}` | Text chunk to process | Document chunker |
| `{entity_context}` | Entity description + aliases | Knowledge graph |
| `{event_list}` | Formatted list of events | Event engine |
| `{claim_text}` | Claim text for evaluation | Extraction engine |
| `{narrative_summary}` | Narrative summary | Narrative engine |
| `{source_info}` | Source name, date, reliability | Provenance model |
| `{instructions}` | Task-specific instructions | Pipeline config |
| `{format_instructions}` | Output format specification | Output schema |

### Context Construction

Prompts are constructed with attention to:
1. **Instruction placement:** System prompt first, then user prompt, then input
2. **Example placement:** Few-shot examples before the actual input
3. **Output format:** Explicit JSON schema in the prompt
4. **Negative instructions:** "If no entities found, return empty array []"
5. **Length management:** Truncate input to fit context window (see `model-strategy.md`)

```rust
pub struct ConstructedPrompt {
    pub system: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: f32,        // 0.1 for extraction (deterministic)
    pub stop_sequences: Vec<String>,
    pub format: OutputFormat,
}

pub fn construct_prompt(template: &PromptTemplate, params: &PromptParams) -> ConstructedPrompt {
    let system = template.system.clone();
    let user = template.user.replace_params(params);
    let messages = vec![
        Message::User { content: user },
    ];

    ConstructedPrompt {
        system,
        messages,
        max_tokens: template.max_output_tokens,
        temperature: template.temperature.unwrap_or(0.1),
        stop_sequences: template.stop_sequences.clone(),
        format: template.output_format.clone(),
    }
}
```

### Structured Output Enforcement

All LLM outputs are validated against schemas:

```rust
pub struct OutputValidation {
    pub is_valid: bool,
    pub parsed: Option<serde_json::Value>,
    pub errors: Vec<String>,
    pub correction_applied: bool,
}

pub fn validate_and_parse(
    raw_output: &str,
    schema: &OutputSchema,
) -> OutputValidation {
    // Step 1: Try direct JSON parse
    let parsed = serde_json::from_str(raw_output);

    // Step 2: If fails, try to extract JSON from markdown/code blocks
    let parsed = parsed.or_else(|_| extract_json_from_markdown(raw_output));

    // Step 3: If still fails, try repair (close unclosed brackets, fix quotes)
    let parsed = parsed.or_else(|_| repair_and_parse(raw_output));

    // Step 4: Validate against schema
    match parsed {
        Ok(value) => {
            let errors = validate_schema(&value, schema);
            OutputValidation {
                is_valid: errors.is_empty(),
                parsed: Some(value),
                errors,
                correction_applied: false,
            }
        }
        Err(e) => {
            OutputValidation {
                is_valid: false,
                parsed: None,
                errors: vec![format!("Parse failed: {}", e)],
                correction_applied: false,
            }
        }
    }
}
```

### Reliability Requirements

| Metric | Target | Degradation Threshold |
|--------|--------|----------------------|
| Parse success rate | > 95% | < 85% triggers alert |
| Empty result rate | < 10% (should rarely return nothing meaningful) | > 25% triggers prompt review |
| Retry rate | < 5% | > 15% triggers prompt review |
| User corrections per 1000 extractions | < 5 | > 20 triggers prompt review |
| Response time (p50, extraction) | < 5s | > 15s triggers model review |
| Response time (p95, report gen) | < 120s | > 300s triggers model review |

**If reliability targets aren't met:**
1. Log detailed failure (prompt, output, error)
2. Prompt review ticket generated
3. A/B test new prompt version against current
4. Rollback or deploy depending on results

### Retry Strategy

```rust
pub enum RetryDecision {
    Retry { attempt: u32, max_retries: u32, backoff: Duration },
    Fallback,       // Use fallback model/parser
    Skip,           // Skip this item
    Abort,          // Stop processing this batch
}

pub fn decide_retry(failure: &InferenceFailure, context: &TaskContext) -> RetryDecision {
    match failure.reason {
        FailureReason::MalformedOutput => {
            if context.attempt < context.max_retries {
                RetryDecision::Retry {
                    attempt: context.attempt + 1,
                    max_retries: context.max_retries,
                    backoff: Duration::from_secs(0), // Immediate retry
                }
            } else {
                RetryDecision::Fallback
            }
        }
        FailureReason::Timeout => {
            RetryDecision::Retry {
                attempt: context.attempt + 1,
                max_retries: 1, // Only retry once on timeout
                backoff: Duration::ZERO,
            }
        }
        FailureReason::EmptyResult => {
            if context.task_type.requires_nonempty() {
                RetryDecision::Retry {
                    attempt: context.attempt + 1,
                    max_retries: 1,
                    backoff: Duration::ZERO,
                }
            } else {
                RetryDecision::Skip // Empty is valid for this task
            }
        }
        FailureReason::ModelUnavailable => {
            RetryDecision::Fallback // Use different model
        }
    }
}
```

### Prompt Quality Monitoring

Metrics tracked per prompt version:

```
prompt_parse_success_rate{name="entity-extraction", version="2"} 0.97
prompt_empty_result_rate{name="entity-extraction", version="2"} 0.03
prompt_retry_rate{name="entity-extraction", version="2"} 0.02
prompt_avg_duration_ms{name="entity-extraction", version="2"} 3200
prompt_user_corrections{name="entity-extraction", version="2"} 4
```

Dashboards:
- Parse success rate over time (per prompt)
- Empty result rate over time
- Retry rate distribution by reason
- Duration percentiles
- Version comparison (A/B test results)
- User correction heatmap (which entities/claims are most often corrected)

### Prompt Testing Framework

Each prompt has an associated test suite:

```yaml
# tests/prompts/entity-extraction.yaml
tests:
  - name: "basic_entity_extraction"
    input: "Apple CEO Tim Cook announced the new iPhone at Apple Park in Cupertino."
    expected:
      - name: "Tim Cook"
        entity_type: "Person"
      - name: "Apple"
        entity_type: "Organization"
      - name: "Apple Park"
        entity_type: "Location"
      - name: "Cupertino"
        entity_type: "Location"
    not_expected:
      - name: "iPhone"  # Product, not entity type we track

  - name: "no_entities"
    input: "It was a sunny day."
    expected: []  # Empty array

  - name: "ambiguous_entity"
    input: "Washington signed the treaty."
    expected:
      - name: "Washington"
        entity_type: "Person"  # Context: "signed the treaty"

run:
  framework: "prompt-test"
  compare:
    baseline: "v1"
    candidate: "v2"
    metric: "f1_score"
    threshold: 0.02  # Must improve F1 by at least 0.02
```

## Interfaces

- `model-strategy.md` — model runtime that executes prompts
- `docs/processing/extraction-engine.md` — uses extraction prompts
- `docs/processing/event-engine.md` — uses generation prompts
- `docs/processing/narrative-engine.md` — uses labeling prompts
- `docs/processing/contradiction-engine.md` — uses evaluation prompts
- `docs/broadcast/broadcast-engine.md` — uses report generation prompts

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Prompt drift (model update changes behavior) | Quality degrades | Weekly prompt quality review; pinned model version |
| Template injection (malicious input in prompt) | Unexpected model behavior | Input sanitization; output validation |
| Overly complex prompt | Token waste, confusion | Prompt length limit; clarity review |
| Few-shot examples mislead model | Systematic errors | Test suite catches regressions |
| Prompt too long for context window | Truncation | Estimate token count; truncate input before prompt |
| Temperature too high for extraction | Hallucination | Use 0.1 for extraction tasks |
| Model can't follow format instructions | Parse failures | Simplify format; use JSON mode if available |

## Future Extensions

- Automated prompt optimization (prompt tuning via gradient-free methods)
- Dynamic few-shot selection (retrieve best examples from history)
- Multi-turn extraction (ask clarifying questions)
- Prompt A/B testing framework with statistical significance
- User feedback integration into prompt improvement loop
- Language-specific prompts for non-English documents
