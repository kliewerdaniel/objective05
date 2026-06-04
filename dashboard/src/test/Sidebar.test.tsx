import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../components/layout/Sidebar'
import { useUiStore } from '../store/uiStore'
import { useFeedStore } from '../store/feedStore'

describe('<Sidebar />', () => {
  beforeEach(() => {
    useUiStore.setState({
      activePage: 'feed',
      sidebarCollapsed: false,
      activeDetailId: null,
      activeDetailType: null,
    })
    useFeedStore.setState({
      health: { status: 'healthy', uptime_secs: 0, services: {} },
    })
  })

  it('renders every primary navigation entry', () => {
    render(<Sidebar />)
    expect(screen.getByText('Live Feed')).toBeInTheDocument()
    expect(screen.getByText('Events')).toBeInTheDocument()
    expect(screen.getByText('Narratives')).toBeInTheDocument()
    expect(screen.getByText('Graph Explorer')).toBeInTheDocument()
    expect(screen.getByText('Broadcasts')).toBeInTheDocument()
    expect(screen.getByText('Sources')).toBeInTheDocument()
    expect(screen.getByText('Settings')).toBeInTheDocument()
  })

  it('marks the active page and switches on click', async () => {
    const user = userEvent.setup()
    const { rerender } = render(<Sidebar />)
    const feedBtn = screen.getByText('Live Feed').closest('button')!
    const eventsBtn = screen.getByText('Events').closest('button')!
    expect(feedBtn.className).toContain('active')
    expect(eventsBtn.className).not.toContain('active')

    await user.click(eventsBtn)
    expect(useUiStore.getState().activePage).toBe('events')
    rerender(<Sidebar />)
    expect(eventsBtn.className).toContain('active')
  });

  it('toggles collapsed state and updates the status label', async () => {
    const user = userEvent.setup()
    render(<Sidebar />)
    // Default state: expanded, status text visible
    expect(screen.getByText('CONNECTED')).toBeInTheDocument()

    // The sidebar header has a single collapse button. Grab it by its
    // class so we don't depend on the lucide-react accessible name.
    const collapseBtn = document.querySelector('.collapse-btn') as HTMLButtonElement
    await user.click(collapseBtn)
    expect(useUiStore.getState().sidebarCollapsed).toBe(true)

    // When collapsed, the status label is hidden
    expect(screen.queryByText('CONNECTED')).toBeNull()
  })
})
