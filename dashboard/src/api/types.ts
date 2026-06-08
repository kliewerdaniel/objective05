export type BodyFormat = 'plain_text' | 'markdown' | 'html';

export interface RawDocument {
  id: string;
  source_id: string;
  source_type: string;
  external_id: string;
  url?: string;
  title?: string;
  body: string;
  body_format: BodyFormat;
  author?: string;
  published_at?: string;
  fetched_at: string;
  language: string;
  content_hash: string;
  metadata: Record<string, unknown>;
}

export type EntityType = 'person' | 'organization' | 'location' | 'concept' | 'event_topic';

export interface ExtractedEntity {
  name: string;
  entity_type: EntityType;
  aliases: string[];
  description?: string;
  metadata: Record<string, unknown>;
  confidence: number;
  evidence_snippet: string;
}

export interface EntitySummary {
  name: string;
  entity_type: string;
  document_count: number;
  confidence: number;
  evidence_snippet: string;
}

export interface EntityDetail {
  entity: ExtractedEntity;
  claims: ExtractedClaim[];
  document_count: number;
}

export interface ExtractedClaim {
  claim_text: string;
  subject_name: string;
  predicate: string;
  object_name?: string;
  object_value?: string;
  claim_type: 'relation' | 'quantification' | 'attribution';
  sentiment?: number;
  confidence: number;
  evidence_snippet: string;
  attributed_to?: string;
}

export interface EventEnvelopeView {
  id: string;
  event_type: string;
  version: number;
  timestamp: string;
  source: string;
  correlation_id: string;
  causation_id: string;
  data: unknown;
  metadata: {
    producer: string;
    producer_version: string;
    retry_count: number;
    produced_at: string;
  };
}

export interface EventRecord {
  subject: string;
  event: EventEnvelopeView;
}

export interface ExtractedRelationship {
  from_entity_name: string;
  to_entity_name: string;
  relationship_type: string;
  confidence: number;
  evidence_snippet: string;
}

export interface ExtractionResult {
  document_id: string;
  entities: ExtractedEntity[];
  claims: ExtractedClaim[];
  relationships: ExtractedRelationship[];
}

export type EventStatus = 'forming' | 'active' | 'evolving' | 'stable' | 'resolved' | 'archived' | 'merged';

export type EventType =
  | 'business'
  | 'politics'
  | 'technology'
  | 'science'
  | 'health'
  | 'world'
  | 'sports'
  | 'entertainment'
  | 'other';

export interface DerivedEvent {
  id: string;
  title: string;
  description: string;
  event_type: EventType;
  status: EventStatus;
  importance: number;
  confidence: number;
  source_diversity: number;
  claim_count: number;
  participating_entities: string[];
  location?: string;
  first_observed_at: string;
  last_updated_at: string;
  last_claim_at: string;
  claim_ids: string[];
  document_ids: string[];
  metadata: Record<string, unknown>;
}

export interface Narrative {
  id: string;
  title: string;
  description: string;
  event_count: number;
  status: string;
  created_at: string;
  strength?: number; // 0 to 1
  sources_count?: number;
  claim_ids?: string[];
  updated_at?: string;
}

export interface Contradiction {
  id: string;
  claim_a: string;
  claim_b: string;
  entity_name: string;
  confidence: number;
  status: string;
  detected_at: string;
  severity?: number; // 0 to 1
  resolved_at?: string | null;
  resolution_note?: string | null;
}

export interface Broadcast {
  id: string;
  title: string;
  summary: string;
  status: string;
  event_count: number;
  created_at: string;
  duration_seconds?: number;
  audio_url?: string;
  content_markdown?: string;
  body_markdown?: string;
  updated_at?: string;
}

export interface SourceInfo {
  source_id: string;
  source_type: string;
  document_count: number;
  extraction_count: number;
  last_fetched_at?: string;
  status?: 'active' | 'error' | 'disabled';
}

export type SourceType =
  | 'rss'
  | 'reddit'
  | 'youtube'
  | 'hackernews'
  | 'arxiv'
  | 'web'
  | 'podcast'
  | 'github'
  | 'githubreleases'
  | 'secedgar'
  | 'static';

export interface SourceDefinition {
  name: string;
  source_type: SourceType;
  url?: string;
  schedule?: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface SourcePatch {
  url?: string;
  schedule?: string;
  enabled?: boolean;
}

export interface SourceRegistryListResponse {
  sources: SourceDefinition[];
  total: number;
}

export interface SourceRegistryEntryResponse {
  source: SourceDefinition;
  message: string;
}

export interface SourceRegistryTriggerResponse {
  source_name: string;
  documents_ingested: number;
  message: string;
}

export interface SourceRegistryError {
  error: string;
  code?: string;
}

// ---- Narratives ---------------------------------------------------------

export type NarrativeStatus = 'forming' | 'active' | 'stable' | 'fading' | 'archived';

export interface NarrativesResponse {
  narratives: Narrative[];
  total: number;
}

export interface NarrativeDetailResponse {
  narrative: Narrative;
}

// ---- Broadcasts ---------------------------------------------------------

export type BroadcastStatus = 'draft' | 'ready' | 'published' | 'archived';

export interface BroadcastsResponse {
  broadcasts: Broadcast[];
  total: number;
}

export interface BroadcastDetailResponse {
  broadcast: Broadcast;
}

export interface BroadcastLatestResponse {
  broadcast: Broadcast | null;
  message: string;
}

export interface BroadcastGenerateRequest {
  title?: string;
  focus?: string;
}

// ---- Contradictions -----------------------------------------------------

export type ContradictionStatus = 'open' | 'investigating' | 'resolved' | 'dismissed';

export interface ContradictionsResponse {
  contradictions: Contradiction[];
  total: number;
}

export interface ContradictionDetailResponse {
  contradiction: Contradiction;
}

export interface ContradictionResolveRequest {
  status?: ContradictionStatus;
  note?: string;
}

// ---- Entities / merge ---------------------------------------------------

export interface EntityMergeRequest {
  source: string;
  target: string;
}

export interface EntityMergeResponse {
  source: string;
  target: string;
  entities_merged: number;
  claims_rewritten: number;
  relationships_rewritten: number;
  message: string;
}

// ---- Events / resolve ---------------------------------------------------

export interface EventResolveRequest {
  note?: string;
}

export interface EventResolveResponse {
  event: Record<string, unknown>;
  message: string;
}

// ---- Search / export / config -------------------------------------------

export interface SearchHit {
  document_id: string;
  source_id: string;
  title?: string;
  url?: string;
  snippet: string;
}

export interface SearchEntityHit {
  name: string;
  entity_type: string;
  snippet: string;
}

export interface SearchClaimHit {
  document_id: string;
  subject_name: string;
  predicate: string;
  object_name: string | null;
  claim_text: string;
}

export interface SearchResponse {
  query: string;
  document_hits: SearchHit[];
  entity_hits: SearchEntityHit[];
  claim_hits: SearchClaimHit[];
  total_hits: number;
}

export interface ConfigResponse {
  data_root: string;
  rest_port: number;
  websocket_port: number;
  cors_allowed_origins: string[];
  auth_enabled: boolean;
  database_path: string;
  document_path: string;
  vector_path: string;
  queue_path: string;
  graph_path: string;
  embedding_path: string;
  nats_url: string;
  use_embedded_nats: boolean;
  log_level: string;
}

export interface PipelineMetrics {
  uptime_secs: number;
  documents_ingested: number;
  extractions_completed: number;
  events_created: number;
  events_merged: number;
  events_dropped: number;
  pipeline_cycles: number;
  retries_attempted: number;
  dead_letters: number;
  snapshots_created: number;
  errors: number;
  started_at: string;
  last_activity_at: string;
}

export interface HealthResponse {
  status: string;
  uptime_secs: number;
  services: Record<string, string>;
}

// ---- Plugins --------------------------------------------------------------

export type PluginType = 'source' | 'processor' | 'broadcast' | 'notification' | 'embedding' | 'filter';
export type PluginState = 'discovered' | 'validated' | 'started' | 'ready' | 'running' | 'crashed' | 'stopped' | 'error';

export interface PluginSubscriptions {
  event_types: string[];
  entity_types: string[];
}

export interface PluginStatusResponse {
  name: string;
  version: string;
  plugin_type: PluginType;
  description: string;
  state: PluginState;
  events_handled: number;
  restart_count: number;
  last_event_at: string | null;
  last_error: string | null;
  started_at: string | null;
  subscriptions: PluginSubscriptions;
  capabilities: string[];
}

export interface PluginsListResponse {
  plugins: PluginStatusResponse[];
  total: number;
}

export interface PluginReloadResponse {
  reloaded: number;
  plugin_names: string[];
}

export interface PluginErrorResponse {
  error: string;
  message: string;
}

// ---- Model Runtime --------------------------------------------------------

export interface SlotSummary {
  path: string;
  path_exists: boolean;
}

export interface StrategyRow {
  kind: string;
  slot: string;
  fallback: string;
}

export type ModelSlotPhase = 'not_loaded' | 'loading' | 'ready' | 'busy' | 'draining' | 'error' | 'unloading';

export interface ModelSlotBusyData {
  active: number;
  queued: number;
}

export interface ModelSlotDrainingData {
  active: number;
}

export interface ModelSlotErrorData {
  message: string;
}

export interface ModelSlotTransition {
  from: Record<string, unknown>;
  to: Record<string, unknown>;
  at: string;
  reason: string | null;
}

export interface SlotViewResponse {
  model: string;
  path: string | null;
  state: Record<string, unknown>;
  max_concurrency: number;
  active: number;
  queued: number;
  last_error: string | null;
  last_used_at: string | null;
  transitions: ModelSlotTransition[];
}

export interface LatencyHistogram {
  count: number;
  sum_ms: number;
  min_ms: number;
  max_ms: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  timeouts: number;
  errors: number;
  buckets: number[];
}

export interface RuntimeMetricsSummary {
  total_calls: number;
  total_fallbacks: number;
  by_kind: Record<string, LatencyHistogram>;
  by_model: Record<string, LatencyHistogram>;
}

export interface ModelRuntimeResponse {
  provider: string;
  embedding_slot: SlotSummary | null;
  extraction_llm_slot: SlotSummary | null;
  strategy: StrategyRow[];
  chunk_timeout_ms: number;
  queue_timeout_ms: number;
  context_window: number;
  max_concurrency: number;
  slots: SlotViewResponse[];
  metrics: RuntimeMetricsSummary | null;
}

export interface ModelRuntimeReloadResponse {
  reloaded: boolean;
  strategy_entries: number;
  provider: string;
  view: ModelRuntimeResponse;
}

export interface ModelRuntimeErrorResponse {
  error: string;
  message: string;
}

// ---- Derived Events Top ---------------------------------------------------

export interface DerivedEventsResponse {
  events: DerivedEvent[];
  count: number;
}

// ---- Recovery -------------------------------------------------------------

export type ServiceStatus = 'healthy' | 'stalled' | 'degraded' | 'recovering' | 'recovered';

export interface RecoveryEvent {
  event_type: string;
  timestamp: string;
  reason: string;
  status: ServiceStatus;
  previous_status: ServiceStatus | null;
}

export interface RecoveryState {
  service_name: string;
  current_status: ServiceStatus;
  last_check_at: string;
  last_heartbeat_at: string | null;
  last_crash_at: string | null;
  last_recovery_at: string | null;
  checks_performed: number;
  crashes_detected: number;
  recoveries_performed: number;
  history: RecoveryEvent[];
}

export interface RecoveryResponse {
  state: RecoveryState | null;
  message: string;
}

export interface RecoveryCheck {
  status: ServiceStatus;
  reason: string;
  stalled: boolean;
  errors_in_window: number;
  seconds_since_activity: number;
  published_crash: boolean;
  published_recovered: boolean;
  published_heartbeat: boolean;
  timestamp: string;
}

export interface RecoveryCheckResponse {
  check: RecoveryCheck;
}

export interface RecoveryErrorResponse {
  error: string;
}
