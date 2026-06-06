// feedStore integration tests.
//
// The store is the dashboard's data layer: it talks to the api
// client, normalises responses, and falls back to curated mock data
// when the backend returns empty lists. These tests assert that
// contract.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useFeedStore } from './feedStore';

// Replace the api module with a configurable mock so tests can
// script responses without going near the network.
vi.mock('../api/client', () => ({
  api: {
    getDocuments: vi.fn(),
    getExtractions: vi.fn(),
    getEvents: vi.fn(),
    getDerivedEvents: vi.fn(),
    getEntities: vi.fn(),
    getEntitySummary: vi.fn(),
    getEntity: vi.fn(),
    getClaims: vi.fn(),
    getNarratives: vi.fn(),
    getContradictions: vi.fn(),
    getBroadcasts: vi.fn(),
    getSources: vi.fn(),
    getMonitoring: vi.fn(),
    getHealth: vi.fn(),
    listRegisteredSources: vi.fn(),
    getRegisteredSource: vi.fn(),
    createSource: vi.fn(),
    updateSource: vi.fn(),
    deleteSource: vi.fn(),
    triggerSource: vi.fn(),
  },
}));

import { api } from '../api/client';
const mockedApi = api as unknown as Record<string, ReturnType<typeof vi.fn>>;

const reset = () => {
  useFeedStore.setState({
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
  });
  for (const fn of Object.values(mockedApi)) {
    (fn as ReturnType<typeof vi.fn>).mockReset();
  }
};

describe('feedStore', () => {
  beforeEach(reset);
  afterEach(reset);

  it('falls back to mock narratives when the backend returns an empty list', async () => {
    mockedApi.getDocuments.mockResolvedValue([]);
    mockedApi.getExtractions.mockResolvedValue([]);
    mockedApi.getEvents.mockResolvedValue([]);
    mockedApi.getDerivedEvents.mockResolvedValue([]);
    mockedApi.getNarratives.mockResolvedValue({ narratives: [], total: 0 });
    mockedApi.getContradictions.mockResolvedValue({ contradictions: [], total: 0 });
    mockedApi.getBroadcasts.mockResolvedValue({ broadcasts: [], total: 0 });
    mockedApi.getSources.mockResolvedValue({ sources: [], total_sources: 0 });
    mockedApi.getMonitoring.mockResolvedValue({ metrics: null, message: 'no data' });
    mockedApi.getHealth.mockResolvedValue({ status: 'healthy', uptime_secs: 0 });
    mockedApi.listRegisteredSources.mockRejectedValue(new Error('no registry'));

    await useFeedStore.getState().fetchData();

    const state = useFeedStore.getState();
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
    expect(state.narratives.length).toBeGreaterThan(0);
    expect(state.contradictions.length).toBeGreaterThan(0);
    expect(state.broadcasts.length).toBeGreaterThan(0);
    expect(state.registryAvailable).toBe(false);
  });

  it('prefers server narratives when the backend returns data', async () => {
    const serverNarrative = {
      id: 'server-1',
      title: 'Server narrative',
      description: 'desc',
      status: 'active',
      event_count: 1,
      claim_ids: [],
      created_at: '2026-06-05T00:00:00Z',
      updated_at: '2026-06-05T00:00:00Z',
    };
    mockedApi.getDocuments.mockResolvedValue([]);
    mockedApi.getExtractions.mockResolvedValue([]);
    mockedApi.getEvents.mockResolvedValue([]);
    mockedApi.getDerivedEvents.mockResolvedValue([]);
    mockedApi.getNarratives.mockResolvedValue({ narratives: [serverNarrative], total: 1 });
    mockedApi.getContradictions.mockResolvedValue({ contradictions: [], total: 0 });
    mockedApi.getBroadcasts.mockResolvedValue({ broadcasts: [], total: 0 });
    mockedApi.getSources.mockResolvedValue({ sources: [], total_sources: 0 });
    mockedApi.getMonitoring.mockResolvedValue({ metrics: null, message: 'no data' });
    mockedApi.getHealth.mockResolvedValue({ status: 'healthy', uptime_secs: 0 });
    mockedApi.listRegisteredSources.mockRejectedValue(new Error('no registry'));

    await useFeedStore.getState().fetchData();

    const state = useFeedStore.getState();
    expect(state.narratives).toHaveLength(1);
    expect(state.narratives[0].id).toBe('server-1');
  });

  it('flags the registry as available when listRegisteredSources resolves', async () => {
    mockedApi.getDocuments.mockResolvedValue([]);
    mockedApi.getExtractions.mockResolvedValue([]);
    mockedApi.getEvents.mockResolvedValue([]);
    mockedApi.getDerivedEvents.mockResolvedValue([]);
    mockedApi.getNarratives.mockResolvedValue({ narratives: [], total: 0 });
    mockedApi.getContradictions.mockResolvedValue({ contradictions: [], total: 0 });
    mockedApi.getBroadcasts.mockResolvedValue({ broadcasts: [], total: 0 });
    mockedApi.getSources.mockResolvedValue({ sources: [], total_sources: 0 });
    mockedApi.getMonitoring.mockResolvedValue({ metrics: null, message: 'no data' });
    mockedApi.getHealth.mockResolvedValue({ status: 'healthy', uptime_secs: 0 });
    mockedApi.listRegisteredSources.mockResolvedValue({
      sources: [
        {
          name: 'hackernews_front',
          source_type: 'rss',
          url: 'https://hnrss.org/frontpage',
          schedule: null,
          enabled: true,
          created_at: '2026-06-05T00:00:00Z',
          updated_at: '2026-06-05T00:00:00Z',
        },
      ],
      total: 1,
    });

    await useFeedStore.getState().fetchData();

    const state = useFeedStore.getState();
    expect(state.registryAvailable).toBe(true);
    expect(state.registeredSources).toHaveLength(1);
    expect(state.registeredSources[0].name).toBe('hackernews_front');
  });

  it('addLiveEvent flips health to healthy on a system event', () => {
    useFeedStore.setState({
      health: { status: 'degraded', uptime_secs: 1, services: {} },
    });

    useFeedStore.getState().addLiveEvent({ type: 'event', event_type: 'system.heartbeat' });
    expect(useFeedStore.getState().health?.status).toBe('healthy');
  });

  it('refreshRegisteredSources updates the registry slice', async () => {
    mockedApi.listRegisteredSources.mockResolvedValue({
      sources: [
        {
          name: 'lobsters',
          source_type: 'rss',
          url: 'https://lobste.rs/rss',
          schedule: null,
          enabled: true,
          created_at: '2026-06-05T00:00:00Z',
          updated_at: '2026-06-05T00:00:00Z',
        },
      ],
      total: 1,
    });

    await useFeedStore.getState().refreshRegisteredSources();
    const state = useFeedStore.getState();
    expect(state.registryAvailable).toBe(true);
    expect(state.registeredSources.map((s) => s.name)).toEqual(['lobsters']);
  });
});
