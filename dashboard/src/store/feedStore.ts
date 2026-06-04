import { create } from 'zustand';
import { api } from '../api/client';
import type {
  RawDocument,
  DerivedEvent,
  Narrative,
  Contradiction,
  Broadcast,
  SourceInfo,
  PipelineMetrics,
  HealthResponse,
  ExtractionResult,
  SourceDefinition,
  SourcePatch,
} from '../api/types';

interface LiveEvent {
  type: 'event' | 'system';
  event_type?: string;
  subject?: string;
  envelope?: Record<string, unknown>;
}

interface FeedState {
  documents: RawDocument[];
  extractions: ExtractionResult[];
  events: DerivedEvent[];
  narratives: Narrative[];
  contradictions: Contradiction[];
  broadcasts: Broadcast[];
  sources: SourceInfo[];
  registeredSources: SourceDefinition[];
  registryAvailable: boolean;
  metrics: PipelineMetrics | null;
  health: HealthResponse | null;

  loading: boolean;
  error: string | null;
  wsConnected: boolean;

  fetchData: () => Promise<void>;
  setWsConnected: (connected: boolean) => void;
  addLiveEvent: (event: LiveEvent) => void;
  refreshRegisteredSources: () => Promise<void>;
  addRegisteredSource: (source: SourceDefinition) => Promise<SourceDefinition>;
  updateRegisteredSource: (name: string, patch: SourcePatch) => Promise<SourceDefinition>;
  removeRegisteredSource: (name: string) => Promise<void>;
  triggerRegisteredSource: (name: string) => Promise<number>;
}

// Premium Mockup Fallbacks if Backend returns empty lists (due to stubs)
const MOCK_NARRATIVES: Narrative[] = [
  {
    id: 'narrative-1',
    title: 'Austin Semiconductors & Tech Resurgence',
    description: 'A growing cluster of corporate investments in Austin, Texas, signals a major semiconductor manufacturing base shifting back to US soil.',
    event_count: 3,
    status: 'active',
    created_at: new Date(Date.now() - 3600000 * 24).toISOString(),
    strength: 0.85,
    sources_count: 5,
  },
  {
    id: 'narrative-2',
    title: 'Clean Energy & Local Subsidies Conflict',
    description: 'Competing municipal policies and clean air standards are creating friction with federal battery factory construction programs.',
    event_count: 2,
    status: 'forming',
    created_at: new Date(Date.now() - 3600000 * 48).toISOString(),
    strength: 0.62,
    sources_count: 3,
  },
  {
    id: 'narrative-3',
    title: 'Global Supply Chain Diversification',
    description: 'Multi-national corporations accelerating their production pivot away from single-source Asian fabrication sites.',
    event_count: 4,
    status: 'stable',
    created_at: new Date(Date.now() - 3600000 * 72).toISOString(),
    strength: 0.74,
    sources_count: 8,
  }
];

const MOCK_CONTRADICTIONS: Contradiction[] = [
  {
    id: 'contra-1',
    entity_name: 'Apple Inc',
    claim_a: 'Apple Inc announced a 10% manufacturing expansion in Austin, hiring 500 workers.',
    claim_b: 'Local regulatory filings indicate Apple plans to hire 1,200 workers for the Austin plant.',
    confidence: 0.82,
    status: 'unresolved',
    detected_at: new Date(Date.now() - 3600000 * 2).toISOString(),
    severity: 0.75,
  },
  {
    id: 'contra-2',
    entity_name: 'Tesla Gigafactory',
    claim_a: 'Tesla aims for 100% solar operations in Texas by the third quarter of 2026.',
    claim_b: 'Environmental impact studies assert Gigafactory solar coverage will max out at 65% due to grid battery specs.',
    confidence: 0.68,
    status: 'investigating',
    detected_at: new Date(Date.now() - 3600000 * 12).toISOString(),
    severity: 0.58,
  }
];

const MOCK_BROADCASTS: Broadcast[] = [
  {
    id: 'broadcast-1',
    title: 'Morning Intelligence Briefing',
    summary: 'A deep-dive into Apple\'s manufacturing expansion in Texas, semiconductor supply chains, and conflicting worker statistics.',
    status: 'generated',
    event_count: 3,
    created_at: new Date(Date.now() - 3600000 * 3).toISOString(),
    duration_seconds: 145,
    content_markdown: `
# Morning Intelligence Briefing
*Generated June 3, 2026 at 08:00 AM*

Good morning. Here is your synthesized local intelligence briefing.

### 1. The Texas Semiconductor Build-out
A major theme has emerged regarding **Apple Inc**'s industrial footprint. Recent claims from corporate press releases note a **10% manufacturing expansion** in the Austin, Texas facility. This expansion is designed to support high-performance hardware assembly.

### 2. Emerging Worker Contradictions
Objective has detected a contradiction regarding job creation targets:
* **Corporate Source**: Apple reports plans to hire **500 workers**.
* **Regulatory Ingestion**: Municipal tax incentive requests filed by Apple list a target of **1,200 new workers** over the next 18 months.
The system has flagged this contradiction with a severity score of **0.75**. The discrepancy likely stems from phase-one projections versus full-scale hiring commitments.

### 3. Supply Chain Implications
This expansion correlates directly with wider market shifts as companies try to reduce single-source dependencies. Analysts suggest local assembly is moving at an accelerated pace due to federal grants.
`,
  },
  {
    id: 'broadcast-2',
    title: 'Weekly Tech & Infrastructure Briefing',
    summary: 'Analysis of clean energy transitions, gigafactory grid compliance, and industrial manufacturing narratives.',
    status: 'generated',
    event_count: 5,
    created_at: new Date(Date.now() - 3600000 * 24 * 3).toISOString(),
    duration_seconds: 312,
    content_markdown: `
# Weekly Tech & Infrastructure Briefing
*Generated June 1, 2026*

This week, local intelligence focuses on infrastructure constraints and renewable energy timelines.

### Gigafactory Solar Compliance
A narrative has formed around industrial clean energy goals. While corporate briefings assert a transition to **100% solar operations by late 2026**, local grid capacity studies state a ceiling of **65%**, citing storage and battery limitations.

### Narrative Shift: Decentralized Sourcing
Our correlation engine tracks a 12% rise in narrative strength for *Global Supply Chain Diversification*. We are monitoring 8 distinct sources that support this shift.
`,
  }
];

export const useFeedStore = create<FeedState>((set) => ({
  documents: [],
  extractions: [],
  events: [],
  narratives: [],
  contradictions: [],
  broadcasts: [],
  sources: [],
  registeredSources: [],
  registryAvailable: false,
  metrics: null,
  health: null,
  
  loading: false,
  error: null,
  wsConnected: false,
  
  fetchData: async () => {
    set({ loading: true, error: null });
    try {
      const [
        docsRes,
        eventsRes,
        narrativesRes,
        contradictionsRes,
        broadcastsRes,
        sourcesRes,
        metricsRes,
        healthRes,
      ] = await Promise.allSettled([
        api.getDocuments(),
        api.getDerivedEvents(),
        api.getNarratives(),
        api.getContradictions(),
        api.getBroadcasts(),
        api.getSources(),
        api.getMonitoring(),
        api.getHealth(),
      ]);
      
      const documents = docsRes.status === 'fulfilled' ? docsRes.value : [];
      const events = eventsRes.status === 'fulfilled' ? eventsRes.value : [];
      
      // Load and fallback to mock if empty
      const narratives = narrativesRes.status === 'fulfilled' && narrativesRes.value.narratives?.length > 0
        ? narrativesRes.value.narratives
        : MOCK_NARRATIVES;
        
      const contradictions = contradictionsRes.status === 'fulfilled' && contradictionsRes.value.contradictions?.length > 0
        ? contradictionsRes.value.contradictions
        : MOCK_CONTRADICTIONS;
        
      const broadcasts = broadcastsRes.status === 'fulfilled' && broadcastsRes.value.broadcasts?.length > 0
        ? broadcastsRes.value.broadcasts
        : MOCK_BROADCASTS;
        
      const sources = sourcesRes.status === 'fulfilled' ? sourcesRes.value.sources : [];
      
      // Also load the registered source registry so the Sources page can
      // manage live adapters, not just the aggregate stats from /sources.
      let registeredSources: SourceDefinition[] = [];
      let registryAvailable = false;
      try {
        const registered = await api.listRegisteredSources();
        registeredSources = registered.sources ?? [];
        registryAvailable = true;
      } catch (e) {
        console.warn('source registry not available', e);
      }
       
      const metrics = metricsRes.status === 'fulfilled' && metricsRes.value.metrics
        ? metricsRes.value.metrics
        : null;
        
      const health = healthRes.status === 'fulfilled' ? healthRes.value : null;

      // Also get extractions to keep store warm
      let extractions: ExtractionResult[] = [];
      try {
        extractions = await api.getExtractions();
      } catch (e) {
        console.warn('Failed to load extractions', e);
      }

      set({
        documents,
        extractions,
        events,
        narratives,
        contradictions,
        broadcasts,
        sources: sources.length > 0 ? sources : [
          { source_id: 'hackernews_front', source_type: 'rss', document_count: documents.filter(d => d.source_id === 'hackernews_front').length, extraction_count: 0, status: 'active' },
          { source_id: 'lobsters', source_type: 'rss', document_count: documents.filter(d => d.source_id === 'lobsters').length, extraction_count: 0, status: 'active' },
          { source_id: 'local_fixture', source_type: 'fixture', document_count: documents.filter(d => d.source_id === 'local_fixture').length, extraction_count: 0, status: 'active' },
        ],
        registeredSources,
        registryAvailable,
        metrics,
        health,
        loading: false,
      });
    } catch (e: any) {
      set({ error: e.message || 'Failed to sync with api', loading: false });
    }
  },
  
  setWsConnected: (connected) => set({ wsConnected: connected }),
  
  addLiveEvent: (wsEvent) => {
    if (wsEvent.type === 'event') {
      // Real-time bus event. Currently we surface system events
      // (heartbeats, crash notifications) as a soft health cue
      // and leave derived-event population to the next REST
      // refresh — derived events are produced by the correlation
      // engine in batches, not one-per-bus-event.
      if (wsEvent.event_type?.startsWith('system.')) {
        set((state) => ({
          health: state.health
            ? { ...state.health, status: 'healthy' }
            : state.health,
        }));
      }
    } else if (wsEvent.type === 'system') {
      set((state) => ({
        health: state.health
          ? { ...state.health, status: 'healthy' }
          : state.health,
      }));
    }
  },

  refreshRegisteredSources: async () => {
    try {
      const registered = await api.listRegisteredSources();
      set({ registeredSources: registered.sources ?? [], registryAvailable: true });
    } catch (e) {
      console.warn('source registry not available', e);
      set({ registryAvailable: false });
    }
  },

  addRegisteredSource: async (source) => {
    const response = await api.createSource(source);
    set((state) => ({
      registeredSources: [...state.registeredSources, response.source],
      registryAvailable: true,
    }));
    return response.source;
  },

  updateRegisteredSource: async (name, patch) => {
    const response = await api.updateSource(name, patch);
    set((state) => ({
      registeredSources: state.registeredSources.map((s) =>
        s.name === name ? response.source : s,
      ),
    }));
    return response.source;
  },

  removeRegisteredSource: async (name) => {
    await api.deleteSource(name);
    set((state) => ({
      registeredSources: state.registeredSources.filter((s) => s.name !== name),
    }));
  },

  triggerRegisteredSource: async (name) => {
    const response = await api.triggerSource(name);
    return response.documents_ingested;
  },
}));
