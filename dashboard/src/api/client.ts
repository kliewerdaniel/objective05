import type {
  HealthResponse,
  RawDocument,
  ExtractionResult,
  DerivedEvent,
  EventRecord,
  EntitySummary,
  EntityDetail,
  ExtractedClaim,
  ExtractedEntity,
  NarrativesResponse,
  NarrativeDetailResponse,
  ContradictionsResponse,
  ContradictionDetailResponse,
  ContradictionResolveRequest,
  BroadcastsResponse,
  BroadcastDetailResponse,
  BroadcastLatestResponse,
  BroadcastGenerateRequest,
  SourceInfo,
  PipelineMetrics,
  SourceDefinition,
  SourcePatch,
  SourceRegistryEntryResponse,
  SourceRegistryListResponse,
  SourceRegistryTriggerResponse,
  EntityMergeRequest,
  EntityMergeResponse,
  EventResolveRequest,
  EventResolveResponse,
  SearchResponse,
  ConfigResponse,
} from './types';

const API_BASE = '/api/v1';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const url = `${API_BASE}${path}`;
  const response = await fetch(url, options);
  if (!response.ok) {
    const body = await response.text();
    let detail = body;
    try {
      const parsed = JSON.parse(body);
      detail = parsed.error || JSON.stringify(parsed);
    } catch {
      // body was not JSON
    }
    throw new Error(`API Error ${response.status} at ${path}: ${detail}`);
  }
  return response.json() as Promise<T>;
}

export const api = {
  getHealth: () => request<HealthResponse>('/health'),
  getStats: () => request<{ documents: number; extractions: number; events: number }>('/stats'),
  getDocuments: () => request<RawDocument[]>('/documents'),
  getExtractions: () => request<ExtractionResult[]>('/extractions'),
  getEvents: () => request<{ events: EventRecord[] }>('/events'),

  getDerivedEvents: () => request<DerivedEvent[]>('/derived-events'),
  getDerivedEvent: (id: string) => request<DerivedEvent>(`/derived-events/${id}`),

  getEntities: () => request<{ entities: ExtractedEntity[] }>('/entities'),
  getEntitySummary: () => request<{ entities: EntitySummary[] }>('/entities/summary'),
  getEntity: (name: string) => request<EntityDetail>(`/entities/${name}`),

  getClaims: () => request<{ claims: ExtractedClaim[] }>('/claims'),
  
  getSources: () => request<{ sources: SourceInfo[]; total_sources: number }>('/sources'),
  
  getMonitoring: () => request<{ metrics: PipelineMetrics | null; message: string }>('/monitoring'),

  // Source registry
  listRegisteredSources: () => request<SourceRegistryListResponse>('/source-registry'),
  getRegisteredSource: (name: string) => request<SourceRegistryEntryResponse>(`/source-registry/${encodeURIComponent(name)}`),
  createSource: (source: SourceDefinition) =>
    request<SourceRegistryEntryResponse>('/source-registry', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(source),
    }),
  updateSource: (name: string, patch: SourcePatch) =>
    request<SourceRegistryEntryResponse>(`/source-registry/${encodeURIComponent(name)}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(patch),
    }),
  deleteSource: (name: string) =>
    request<SourceRegistryEntryResponse>(`/source-registry/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),
  triggerSource: (name: string) =>
    request<SourceRegistryTriggerResponse>(`/source-registry/${encodeURIComponent(name)}/trigger`, {
      method: 'POST',
    }),

  // Narratives
  getNarratives: () => request<NarrativesResponse>('/narratives'),
  getNarrative: (id: string) => request<NarrativeDetailResponse>(`/narratives/${encodeURIComponent(id)}`),

  // Broadcasts
  getBroadcasts: () => request<BroadcastsResponse>('/broadcasts'),
  getLatestBroadcast: () => request<BroadcastLatestResponse>('/broadcasts/latest'),
  getBroadcast: (id: string) => request<BroadcastDetailResponse>(`/broadcasts/${encodeURIComponent(id)}`),
  generateBroadcast: (req?: BroadcastGenerateRequest) =>
    request<BroadcastDetailResponse>('/broadcasts/generate', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(req ?? {}),
    }),

  // Contradictions
  getContradictions: () => request<ContradictionsResponse>('/contradictions'),
  getContradiction: (id: string) => request<ContradictionDetailResponse>(`/contradictions/${encodeURIComponent(id)}`),
  resolveContradiction: (id: string, req?: ContradictionResolveRequest) =>
    request<ContradictionDetailResponse>(`/contradictions/${encodeURIComponent(id)}/resolve`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(req ?? {}),
    }),

  // Entities
  mergeEntities: (req: EntityMergeRequest) =>
    request<EntityMergeResponse>('/entities/merge', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(req),
    }),

  // Events
  resolveEvent: (id: string, req?: EventResolveRequest) =>
    request<EventResolveResponse>(`/events/${encodeURIComponent(id)}/resolve`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(req ?? {}),
    }),

  // Search / config
  search: (q: string, limit = 25) =>
    request<SearchResponse>(`/search?q=${encodeURIComponent(q)}&limit=${limit}`),
  getConfig: () => request<ConfigResponse>('/config'),
  exportDataUrl: () => `${API_BASE}/export`,
};
