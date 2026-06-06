// API client error parsing + URL construction.
//
// These tests mock `fetch` directly so we can assert how the
// client surfaces non-2xx responses and how it shapes the URL.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from './client';

const mockFetch = (response: {
  ok: boolean;
  status: number;
  body: string;
  contentType?: string;
}) => {
  return vi.fn().mockResolvedValue({
    ok: response.ok,
    status: response.status,
    text: async () => response.body,
    json: async () => JSON.parse(response.body || 'null'),
    headers: { get: (h: string) => response.contentType ?? h === 'content-type' ? 'application/json' : null },
  });
};

describe('api client', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('uses /api/v1 as the base and parses JSON responses', async () => {
    const fetchMock = mockFetch({
      ok: true,
      status: 200,
      body: JSON.stringify({ status: 'healthy', uptime_secs: 42 }),
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await api.getHealth();
    expect(result.status).toBe('healthy');
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/v1/health');
  });

  it('encodes plugin names in the URL path', async () => {
    const fetchMock = mockFetch({
      ok: true,
      status: 200,
      body: JSON.stringify({ source: { name: 'with space' } }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await api.getRegisteredSource('with space');
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/v1/source-registry/with%20space');
  });

  it('throws with status + parsed JSON error when the response is not ok', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetch({
        ok: false,
        status: 404,
        body: JSON.stringify({ error: 'plugin_not_found', message: "plugin 'foo' is not registered" }),
      }),
    );

    await expect(api.getRegisteredSource('foo')).rejects.toThrow(
      /API Error 404 at \/source-registry\/foo: plugin_not_found/,
    );
  });

  it('falls back to plain-text bodies when the error is not JSON', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetch({
        ok: false,
        status: 503,
        body: 'service unavailable',
      }),
    );

    await expect(api.getHealth()).rejects.toThrow(
      /API Error 503 at \/health: service unavailable/,
    );
  });

  it('serialises JSON bodies for POST/PUT requests', async () => {
    const fetchMock = mockFetch({
      ok: true,
      status: 200,
      body: JSON.stringify({ contradiction: { id: 'c-1' } }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await api.resolveContradiction('c-1', { status: 'resolved', note: 'ok' });
    const [, init] = fetchMock.mock.calls[0];
    expect(init.method).toBe('POST');
    const headers = init.headers as Record<string, string>;
    expect(headers['content-type']).toBe('application/json');
    expect(JSON.parse(init.body)).toEqual({ status: 'resolved', note: 'ok' });
  });

  it('builds a search URL with encoded query and limit', async () => {
    const fetchMock = mockFetch({
      ok: true,
      status: 200,
      body: JSON.stringify({ results: [], total: 0 }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await api.search('austin tx', 5);
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/v1/search?q=austin%20tx&limit=5');
  });

  it('exposes a stable exportDataUrl helper', () => {
    expect(api.exportDataUrl()).toBe('/api/v1/export');
  });
});
