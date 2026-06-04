import { describe, it, expect, beforeEach } from 'vitest'
import { useUiStore } from '../store/uiStore'

describe('uiStore', () => {
  beforeEach(() => {
    // Reset the store between tests so notifications / detail state
    // from one test cannot leak into the next.
    useUiStore.setState({
      activePage: 'feed',
      activeDetailId: null,
      activeDetailType: null,
      sidebarCollapsed: false,
      notifications: [],
    })
  })

  it('starts on the feed page', () => {
    expect(useUiStore.getState().activePage).toBe('feed')
  })

  it('switches active page and clears any open detail', () => {
    useUiStore.getState().openDetail('event', 'event-1')
    useUiStore.getState().setActivePage('narratives')

    const state = useUiStore.getState()
    expect(state.activePage).toBe('narratives')
    expect(state.activeDetailId).toBeNull()
    expect(state.activeDetailType).toBeNull()
  })

  it('opens and closes the detail drawer', () => {
    useUiStore.getState().openDetail('document', 'doc-42')
    expect(useUiStore.getState().activeDetailId).toBe('doc-42')
    expect(useUiStore.getState().activeDetailType).toBe('document')

    useUiStore.getState().closeDetail()
    expect(useUiStore.getState().activeDetailId).toBeNull()
    expect(useUiStore.getState().activeDetailType).toBeNull()
  })

  it('toggles the sidebar collapsed state', () => {
    expect(useUiStore.getState().sidebarCollapsed).toBe(false)
    useUiStore.getState().toggleSidebar()
    expect(useUiStore.getState().sidebarCollapsed).toBe(true)
    useUiStore.getState().toggleSidebar()
    expect(useUiStore.getState().sidebarCollapsed).toBe(false)
  })

  it('adds notifications with a generated id', () => {
    useUiStore.getState().addNotification('Source polled', 'success')
    const list = useUiStore.getState().notifications
    expect(list).toHaveLength(1)
    expect(list[0].message).toBe('Source polled')
    expect(list[0].type).toBe('success')
    expect(typeof list[0].id).toBe('string')
  })

  it('removes a notification by id', () => {
    useUiStore.getState().addNotification('to-keep', 'info')
    useUiStore.getState().addNotification('to-remove', 'info')
    const ids = useUiStore.getState().notifications.map((n) => n.id)
    useUiStore.getState().removeNotification(ids[1])

    const remaining = useUiStore.getState().notifications
    expect(remaining).toHaveLength(1)
    expect(remaining[0].message).toBe('to-keep')
  })
})
