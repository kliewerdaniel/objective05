# Agent Architecture

## Purpose

Define Objective's approach to agents — whether they exist, and if they do, their definitions, responsibilities, and communication patterns. This document addresses the architectural decision about autonomous agents in the system.

## Scope

This document covers the rationale for Objective's agent design, agent definitions (if any), responsibilities, communication patterns, failure handling, and the relationship between agents and other system components.

## Responsibilities

- Document the agent design decision
- Define agent boundaries and responsibilities
- Specify agent communication patterns
- Define agent failure handling
- Document limitations and exclusions

## Assumptions

- The term "agent" needs precise definition to avoid confusion with AI agent hype
- Objective's architecture is pipeline-based, not agent-based
- Some tasks are suitable for agent-like autonomy; most are not
- Agent-like behavior exists only where it adds clear value

## Design

### Decision: Objective Does Not Use Autonomous Agents

After evaluating the architectural requirements, Objective is designed as a **pipeline-based system**, not an agent-based system. Here is the rationale:

**What we mean by "agent":** An autonomous AI system that:
- Sets its own goals or sub-goals
- Decides which tools/functions to call
- Maintains its own state and plan
- Operates without direct human supervision

**Why Objective does not use agents:**

| Concern | Explanation |
|---------|-------------|
| **Determinism** | Objective's processing pipelines must produce consistent, auditable results. Autonomous agents (LLMs choosing their own paths) introduce non-determinism that contradicts reliability requirements. |
| **Reproducibility** | If a bug is found in extraction, the fix must produce consistent results on re-processing. Agent decisions are not reproducible by design. |
| **Debugging complexity** | Agent decision traces are far harder to debug than deterministic pipeline stages with well-defined inputs and outputs. |
| **Resource overhead** | Agents maintain conversational state, run reasoning loops, and call tools — all expensive operations on local hardware. |
| **Unpredictable latency** | Agent reasoning loops have unpredictable completion times, violating Objective's scheduling guarantees. |
| **Failures are opaque** | When an agent fails, it's often unclear why. Pipeline stages report clear error modes. |

**Where agent-like patterns ARE used:**

While Objective does not have autonomous agents, it uses LLM-based processing in well-scoped, deterministic contexts:

1. **Structured extraction** — LLMs extract entities/claims with defined output schemas. The LLM does not choose what to extract or how to format it.
2. **Structured generation** — LLMs generate reports, titles, and summaries from defined inputs. The LLM does not choose the topic or format.
3. **Contradiction evaluation** — LLMs evaluate claim pairs against defined criteria. The LLM does not decide which claims to evaluate.

These are **LLM-as-function-call** patterns, not agent patterns. Every LLM call has:
- A fixed prompt template
- A defined output schema
- A deterministic calling context
- No ability to call tools or make decisions about the processing flow

### If Agents Were to Be Added

The following constraints would apply:

1. **Agents operate within sandboxed pipelines** — No agent may modify system state directly. All agent outputs pass through validation and human approval gates.
2. **Agents have no persistent memory** — Agent state is ephemeral. Any information that needs persistence must be written to the knowledge graph by a non-agent pipeline stage.
3. **Agents have bounded autonomy** — Maximum 3 tool calls before requiring human approval. Tool call budget resets per generation cycle.
4. **Agents are optional** — The core system operates fully without agents. Agents provide additive value only.

### Current Agent-Like Functions

The closest thing to an agent in Objective is the **broadcast generation pipeline**, which has limited autonomy:

```rust
pub struct BroadcastGenerationPipeline {
    steps: Vec<Box<dyn GenerationStep>>,
}

pub trait GenerationStep {
    fn name(&self) -> &str;
    fn execute(&self, context: &mut GenerationContext) -> Result<StepOutput>;
}
```

Steps (in order):
1. `topic_selection` — Choose broadcast topics from ranked events/narratives
2. `fact_gathering` — Query knowledge graph for relevant claims and events
3. `outline_generation` — Generate structured outline (headings, key points)
4. `draft_generation` — Generate full report text per section
5. `fact_verification` — Check generated text against sourced claims
6. `formatting` — Apply broadcast format (brief, full, audio script)
7. `review` — Validate output quality (length, coverage, accuracy)

This is a pipeline, not an agent. Each step has:
- Fixed inputs (from previous step)
- Fixed outputs (to next step)
- No autonomy in step selection or ordering
- Deterministic execution within each step

### If Autonomous Agents Were Requested

If future requirements demand autonomous agents, the following architecture would apply:

**Agent types:**
- **Research Agent:** Given a topic, finds relevant information across sources and graph
- **Deep Dive Agent:** Given an entity or event, produces comprehensive analysis
- **Monitor Agent:** Given a topic, watches for new information and alerts

**Constraints:**
- All agents operate within the plugin system (see `plugin-api.md`)
- Agents communicate via the message bus (same as services)
- Agent outputs go through the same validation pipeline as all extractions
- Agent actions are logged to the audit trail
- Agent resource usage is capped (CPU time, memory, inference tokens)
- Agent autonomy is bounded (max N tool calls per invocation)

**Communication:**
```
Agent → Message Bus → Plugin Host → Core Services
   ↑                                      │
   └──────────────────────────────────────┘
              (via events)
```

## Interfaces

- `docs/architecture/architecture-decisions.md` — why pipeline over agents
- `model-strategy.md` — LLM resources for agent-like functions
- `prompting-architecture.md` — prompt structure for generation
- `docs/broadcast/broadcast-engine.md` — broadcast pipeline (closest to agent)
- `docs/api/plugin-api.md` — plugin system for custom agents

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Agent hallucination (if added) | Incorrect information injected | Output validation gates; source verification step |
| Agent loops (if added) | Resource exhaustion | Max step limit; timeout per agent invocation |
| Agent not needed | Unnecessary complexity | Clear architectural decision documented above |
| Future pressure to add agents | Scope creep | Require concrete use case before adding; start with pipeline extension |

## Future Extensions

- Goal-oriented research agents (Phase 4)
- Custom agent pipeline using plugin system
- Agent output review dashboard (human in the loop)
- Agent capability marketplace (community-developed agents)
- Hierarchical agent orchestration (coordinator + specialist agents)
