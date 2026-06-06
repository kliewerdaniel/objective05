// Header component tests.
//
// Header reads from `useUiStore` and `useFeedStore` and renders
// the page title, the connection indicator, the uptime widget, and
// a manual sync button. We assert that:
//   - the page title tracks `activePage`
//   - the connection indicator shows "Live" when `wsConnected`
//   - the sync button calls `fetchData` and reflects `loading`

import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Header } from './Header';
import { useFeedStore } from '../../store/feedStore';
import { useUiStore } from '../../store/uiStore';

const reset = () => {
  useUiStore.setState({
    activePage: 'feed',
    sidebarCollapsed: false,
    activeDetailId: null,
    activeDetailType: null,
    notifications: [],
  });
  useFeedStore.setState({
    loading: false,
    wsConnected: false,
    metrics: null,
    health: null,
    documents: [],
    extractions: [],
    events: [],
    narratives: [],
    contradictions: [],
    broadcasts: [],
    sources: [],
    registeredSources: [],
    registryAvailable: false,
    error: null,
  });
  vi.restoreAllMocks();
};

describe('Header', () => {
  beforeEach(reset);
  afterEach(reset);

  it('renders the page title from the active page', () => {
    useUiStore.setState({ activePage: 'broadcasts' });
    render(<Header />);
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(
      'Continuous Broadcasts',
    );
  });

  it('shows the Live indicator when the WebSocket is connected', () => {
    useFeedStore.setState({ wsConnected: true });
    render(<Header />);
    expect(screen.getByText('Live')).toBeInTheDocument();
  });

  it('shows the Syncing indicator when the WebSocket is offline', () => {
    useFeedStore.setState({ wsConnected: false });
    render(<Header />);
    expect(screen.getByText('Syncing')).toBeInTheDocument();
  });

  it('formats uptime as hours and minutes from health', () => {
    useFeedStore.setState({
      health: { status: 'healthy', uptime_secs: 3661, services: {} },
    });
    render(<Header />);
    expect(screen.getByText(/Uptime:/)).toHaveTextContent('1h 1m');
  });

  it('falls back to 0m when uptime is unknown', () => {
    useFeedStore.setState({ health: null, metrics: null });
    render(<Header />);
    expect(screen.getByText(/Uptime:/)).toHaveTextContent('0m');
  });

  it('invokes fetchData when the sync button is clicked', () => {
    const fetchData = vi.fn().mockResolvedValue(undefined);
    useFeedStore.setState({ fetchData });
    render(<Header />);
    fireEvent.click(screen.getByRole('button', { name: /Manual Sync/i }));
    expect(fetchData).toHaveBeenCalledTimes(1);
  });

  it('disables the sync button while loading', () => {
    useFeedStore.setState({ loading: true });
    render(<Header />);
    const button = screen.getByRole('button', { name: /Manual Sync/i });
    expect(button).toBeDisabled();
  });
});
