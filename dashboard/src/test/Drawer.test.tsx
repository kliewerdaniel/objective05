import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { DetailDrawer } from '../components/common/DetailDrawer'
import { useUiStore } from '../store/uiStore'
import { useFeedStore } from '../store/feedStore'

describe('<DetailDrawer />', () => {
  beforeEach(() => {
    useUiStore.setState({
      activeDetailId: null,
      activeDetailType: null,
    })
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
    })
  })

  it('renders nothing when no detail is selected', () => {
    const { container } = render(<DetailDrawer />)
    expect(container.firstChild).toBeNull()
  })

  it('renders a not-found message when the document id does not match', () => {
    useUiStore.getState().openDetail('document', 'missing-id')
    render(<DetailDrawer />)
    expect(screen.getByText('Document not found.')).toBeInTheDocument()
  })

  it('renders document body and associated claims', () => {
    useFeedStore.setState({
      documents: [
        {
          id: 'doc-1',
          source_id: 'hackernews_front',
          source_type: 'rss',
          external_id: 'ext-1',
          title: 'Big News',
          body: 'A long article about something interesting.',
          body_format: 'plain_text',
          author: 'Alice',
          fetched_at: new Date().toISOString(),
          language: 'en',
          content_hash: 'h',
          metadata: {},
        },
      ],
      extractions: [
        {
          document_id: 'doc-1',
          entities: [
            { name: 'Apple Inc', entity_type: 'organization', aliases: [], metadata: {}, confidence: 0.9, evidence_snippet: 'Apple' },
          ],
          claims: [
            {
              claim_text: 'Apple announced new chips.',
              subject_name: 'Apple Inc',
              predicate: 'announced',
              object_name: 'M5',
              claim_type: 'relation',
              confidence: 0.8,
              evidence_snippet: 'Apple announced new chips.',
            },
          ],
          relationships: [],
        },
      ],
    })

    useUiStore.getState().openDetail('document', 'doc-1')
    render(<DetailDrawer />)

    expect(screen.getByText('Big News')).toBeInTheDocument()
    expect(screen.getByText('A long article about something interesting.')).toBeInTheDocument()
    expect(screen.getByText('Asserted Claims (1)')).toBeInTheDocument()
    expect(screen.getByText('"Apple announced new chips."')).toBeInTheDocument()
  })

  it('closes the drawer when the overlay is clicked', () => {
    useUiStore.getState().openDetail('event', 'event-1')
    render(<DetailDrawer />)
    const overlay = screen.getByText(/event details/i).closest('.drawer-overlay')!
    fireEvent.click(overlay)
    expect(useUiStore.getState().activeDetailId).toBeNull()
  })
})
