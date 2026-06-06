// uiStore state transitions: page changes, sidebar toggle, detail
// drawer, and notifications (which auto-expire after 4s — covered
// here with fake timers).

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useUiStore } from './uiStore';

describe('uiStore', () => {
  beforeEach(() => {
    useUiStore.setState({
      activePage: 'feed',
      sidebarCollapsed: false,
      activeDetailId: null,
      activeDetailType: null,
      notifications: [],
    });
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('updates activePage and clears any open detail', () => {
    useUiStore.getState().openDetail('event', 'evt-1');
    useUiStore.getState().setActivePage('settings');
    const s = useUiStore.getState();
    expect(s.activePage).toBe('settings');
    expect(s.activeDetailId).toBeNull();
    expect(s.activeDetailType).toBeNull();
  });

  it('toggles the sidebar collapsed state', () => {
    expect(useUiStore.getState().sidebarCollapsed).toBe(false);
    useUiStore.getState().toggleSidebar();
    expect(useUiStore.getState().sidebarCollapsed).toBe(true);
    useUiStore.getState().toggleSidebar();
    expect(useFeedStoreSafe().sidebarCollapsed).toBe(false);
  });

  it('opens and closes the detail drawer', () => {
    useUiStore.getState().openDetail('narrative', 'nar-9');
    let s = useUiStore.getState();
    expect(s.activeDetailType).toBe('narrative');
    expect(s.activeDetailId).toBe('nar-9');
    useUiStore.getState().closeDetail();
    s = useUiStore.getState();
    expect(s.activeDetailType).toBeNull();
    expect(s.activeDetailId).toBeNull();
  });

  it('adds notifications and auto-removes them after 4s', () => {
    useUiStore.getState().addNotification('saved', 'success');
    expect(useUiStore.getState().notifications).toHaveLength(1);
    expect(useUiStore.getState().notifications[0].type).toBe('success');

    vi.advanceTimersByTime(4000);
    expect(useUiStore.getState().notifications).toHaveLength(0);
  });

  it('removes a notification immediately when removeNotification is called', () => {
    useUiStore.getState().addNotification('first');
    useUiStore.getState().addNotification('second');
    expect(useUiStore.getState().notifications).toHaveLength(2);
    const id = useUiStore.getState().notifications[0].id;
    useUiStore.getState().removeNotification(id);
    expect(useUiStore.getState().notifications.map((n) => n.message)).toEqual(['second']);
  });
});

// Small helper to dodge the same destructuring / refactor in the
// toggle test above.
function useFeedStoreSafe() {
  return useUiStore.getState();
}
