import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { useFeedStore } from '../store/feedStore'
import { api } from '../api/client'

// Stub the API client so the store can be exercised without
// hitting a running Rust backend.
vi.mock('../api/client', () => {
  return {
    api: {
      getDocuments: vi.fn(async () => []),
      getExtractions: vi.fn(async () => []),
      getDerivedEvents: vi.fn(async () => []),
      getNarratives: vi.fn(async () => ({ narratives: [], total: 0 })),
      getContradictions: vi.fn(async () => ({ contradictions: [], total: 0 })),
      getBroadcasts: vi.fn(async () => ({ broadcasts: [], total: 0 })),
      getSources: vi.fn(async () => ({ sources: [], total_sources: 0 })),
      getMonitoring: vi.fn(async () => ({ metrics: null, message: '' })),
      getHealth: vi.fn(async () => ({
        status: 'healthy',
        uptime_secs: 0,
        services: {},
      })),
      listRegisteredSources: vi.fn(async () => ({ sources: [], total: 0 })),
    },
  }
})

describe('feedStore', () => {
  beforeEach(() => {
    vi.clearAllMocks()
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
    })
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('fetchData hydrates the store from the API', async () => {
    vi.mocked(api.getDocuments).mockResolvedValueOnce([
      {
        id: 'doc-1',
        source_id: 'hackernews_front',
        source_type: 'rss',
        external_id: 'ext-1',
        body: 'hello world',
        body_format: 'plain_text',
        fetched_at: new Date().toISOString(),
        language: 'en',
        content_hash: 'h',
        metadata: {},
      },
    ])
    vi.mocked(api.getNarratives).mockResolvedValueOnce({
      narratives: [
        {
          id: 'n-1',
          title: 'Mock narrative',
          description: 'test',
          event_count: 1,
          status: 'active',
          created_at: new Date().toISOString(),
        },
      ],
      total: 1,
    })
    vi.mocked(api.getHealth).mockResolvedValueOnce({
      status: 'healthy',
      uptime_secs: 30,
      services: {},
    })

    await useFeedStore.getState().fetchData()

    const state = useFeedStore.getState()
    expect(state.documents).toHaveLength(1)
    expect(state.narratives).toHaveLength(1)
    expect(state.health?.status).toBe('healthy')
    expect(state.loading).toBe(false)
  })

  it('falls back to mock narratives when the API returns an empty list', async () => {
    vi.mocked(api.getNarratives).mockResolvedValueOnce({
      narratives: [],
      total: 0,
    })
    await useFeedStore.getState().fetchData()
    // The store ships with curated mock narratives as a developer
    // convenience; an empty server response should not replace them
    // with an empty list (smoke test that the fallback fires).
    expect(useFeedStore.getState().narratives.length).toBeGreaterThan(0)
  })

  it('refreshRegisteredSources marks the registry available on success', async () => {
    vi.mocked(api.listRegisteredSources).mockResolvedValueOnce({
      sources: [
        {
          name: 'hnrss',
          source_type: 'rss',
          url: 'https://hnrss.org/frontpage',
          enabled: true,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      ],
      total: 1,
    })
    await useFeedStore.getState().refreshRegisteredSources()
    const state = useFeedStore.getState()
    expect(state.registeredSources).toHaveLength(1)
    expect(state.registryAvailable).toBe(true)
  })

  it('refreshRegisteredSources marks the registry unavailable on failure', async () => {
    vi.mocked(api.listRegisteredSources).mockRejectedValueOnce(
      new Error('503 Service Unavailable'),
    )
    await useFeedStore.getState().refreshRegisteredSources()
    expect(useFeedStore.getState().registryAvailable).toBe(false)
  })

  it('tracks websocket state', () => {
    useFeedStore.getState().setWsConnected(true)
    expect(useFeedStore.getState().wsConnected).toBe(true)
    useFeedStore.getState().setWsConnected(false)
    expect(useFeedStore.getState().wsConnected).toBe(false)
  })
})
