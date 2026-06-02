# Dashboard Specification

## Purpose

Define the dashboard UI specification — the primary human interface for interacting with Objective.

## Scope

This document covers dashboard layout, navigation structure, event explorer, narrative explorer, graph explorer, broadcast viewer, source management, configuration panels, and real-time updates.

## Responsibilities

- Define dashboard layout and component hierarchy
- Specify navigation structure
- Define data visualization requirements
- Document real-time update patterns
- Specify accessibility requirements
- Define information density guidelines

## Assumptions

- Dashboard is a web application (single-page app)
- Communication with core via REST API and WebSocket
- Dashboard runs in modern browsers (Chrome, Firefox, Safari, Edge)
- Dashboard is served by Objective's API gateway
- No authentication for MVP (local-first, single-user)

## Design

### Technology Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Framework | React + TypeScript | Broad ecosystem, type safety |
| State management | Zustand | Lightweight, simple |
| Graph visualization | D3.js / vis-network | Flexible, interactive |
| Real-time | WebSocket (native) | Direct connection to API gateway |
| Styling | Tailwind CSS | Utility-first, fast iteration |
| Charts | Recharts / visx | Simple charting |
| Build | Vite | Fast dev, small bundles |

### Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  ┌──────┐  ┌──────────────────────────────────────────────────────┐│
│  │      │  │  Header: Objective | Status: Healthy | 12:34 PM      ││
│  │ Side │  ├──────────────────────────────────────────────────────┤│
│  │ nav  │  │                                                      ││
│  │      │  │              Main Content Area                       ││
│  │ ─────│  │                                                      ││
│  │ Feed │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐           ││
│  │ ─────│  │  │ Widget 1  │  │ Widget 2  │  │ Widget 3  │        ││
│  │ Evnts│  │  └──────────┘  └──────────┘  └──────────┘           ││
│  │ ─────│  │                                                      ││
│  │ Narr │  │  ┌────────────────────────────────────────────┐      ││
│  │ ─────│  │  │                                            │      ││
│  │ Grph │  │  │         Main list / detail view            │      ││
│  │ ─────│  │  │                                            │      ││
│  │ Brdc │  │  └────────────────────────────────────────────┘      ││
│  │ ─────│  │                                                      ││
│  │ Srcs │  └──────────────────────────────────────────────────────┤│
│  │ ─────│  │  Footer: Last updated | X sources | Y broadcasts     ││
│  │ Cfg  │  └──────────────────────────────────────────────────────┘│
│  │      │                                                          │
│  └──────┘                                                          │
└─────────────────────────────────────────────────────────────────────┘
```

### Navigation

**Sidebar navigation:**

| Item | Icon | Route | Description |
|------|------|-------|-------------|
| Feed | `⊗` | `/feed` | Live event feed (default page) |
| Events | `◈` | `/events` | Event explorer |
| Narratives | `⊡` | `/narratives` | Narrative explorer |
| Graph | `◉` | `/graph` | Knowledge graph visualization |
| Broadcasts | `▶` | `/broadcasts` | Broadcast viewer |
| Sources | `⊕` | `/sources` | Source management |
| Configuration | `⚙` | `/settings` | System settings |

### Pages

#### Feed Page (`/feed`)

Real-time activity feed showing all system activity:

- Filter: event type pills (All, Events, Narratives, Contradictions, System)
- Sort: newest first
- Each item: icon + title + timestamp + brief description
- Click → navigates to detail view
- Auto-scroll: new items appear at top with "N new items" banner
- Virtual scrolling for performance

**Feed item types:**
```
📰 New document ingested: "Article Title" (source)
🏷 Entity extracted: "Entity Name" (Type)
📝 New claim: "Claim text" (confidence: 0.92)
🔷 New event formed: "Event Title" (importance: 0.78)
📊 Narrative updated: "Narrative Title" (strength: 0.65)
⚡ Contradiction detected: "Description" (severity: 0.8)
📡 Broadcast generated: "Morning Briefing"
```

#### Events Page (`/events`)

Event explorer with filtering and sorting:

- List view: cards with title, type, importance, confidence, date, claim count
- Filter bar: status, type, importance range, date range, search text
- Sort: importance, date created, last updated, confidence
- Pagination: 20 per page, infinite scroll
- Click → event detail slide-in panel

**Event detail panel:**
- Title, type, status badges
- Importance gauge (0-1 horizontal bar)
- Timeline of claims (chronological)
- Entity list (clickable → entity detail)
- Source list (documents supporting this event)
- Related narratives (links to narrative detail)
- Contradictions involving this event
- Action buttons: Resolve, Merge, Export

#### Narratives Page (`/narratives`)

Narrative explorer:

- Card view: title, strength gauge, event count, source count, status badge
- Color-coded by status: Forming (yellow), Active (green), Mature (blue), Declining (orange), Archived (gray)
- Filter: status, type, strength range
- Click → narrative detail

**Narrative detail panel:**
- Title, summary, type, status
- Strength breakdown (coverage, diversity, recency, momentum) — radar chart or bar chart
- Event timeline (events in narrative ordered by time)
- Event cards within narrative (clickable → event detail)
- Relationship tree (parent, child, sibling narratives)
- Source distribution (pie chart or bar chart of sources)
- Contradictions within this narrative

#### Graph Page (`/graph`)

Knowledge graph visualization:

- Force-directed graph layout
- Node types: Entity (circles), Event (diamonds), Narrative (hexagons)
- Edge types: different colors for different relationship types (ENTITY_RELATED_TO, CLAIM_SUPPORTS_EVENT, EVENT_BELONGS_TO)
- Interaction: drag, zoom, pan, click for details, hover for labels
- Search: search for entity/event/narrative by name
- Filter: filter by node type, edge type, date range
- Focus: double-click node to center and show 1-hop neighbors
- Minimap: small overview in corner
- Controls: zoom in/out, reset view, fit to screen, toggle labels, toggle physics
- Performance: max 500 nodes displayed; larger graphs show clustered view

**Entity detail on graph click:**
- Slide-in panel with entity name, type, description
- Mention count, confidence
- Top claims involving this entity
- Related entities (list)
- Events entity participates in
- Sources that mention this entity

#### Broadcasts Page (`/broadcasts`)

Broadcast viewer:

- List: date/time, title, format, duration, word count
- Click → broadcast reader (rendered markdown)
- Audio player for generated audio
- Archive browsing (by date, search)
- Download: text (MD/HTML), audio (MP3)

**Broadcast reader:**
- Formatted markdown with headings, bullet points
- Source citations (clickable → document in feed)
- Segment navigation (previous/next section)
- Audio sync: highlight text as audio plays (if available)
- Share/export buttons

#### Sources Page (`/sources`)

Source management dashboard:

- List: source name, type, status (Active/Error/Disabled), last poll, documents fetched
- Status indicators: green (healthy), yellow (degraded), red (error)
- Click → source detail with metrics:
  - Poll count, error count, documents fetched
  - Success rate over time (mini chart)
  - Error log
- Add source: dialog with type selector, configuration form per type
- Edit source: modify configuration
- Delete source: confirm dialog
- Test source: immediate fetch to verify configuration

#### Settings Page (`/settings`)

System configuration:

- Tabs: General, Models, Broadcast, Audio, Storage, Security
- General: data directory, language, log level, auto-start
- Models: model registry, download/update, per-task model selection
- Broadcast: schedule configuration, format preferences, breaking news settings
- Audio: TTS engine, voice selection, audio quality settings
- Storage: data retention settings, snapshot schedule, backup triggers
- Security: encryption settings (future), API authentication (future)

### Visual Design Principles

- Dark theme default (light theme optional)
- High information density (data-rich views)
- Consistent color coding:
  - Person: #4A90D9 (blue)
  - Organization: #7B61FF (purple)
  - Location: #50C878 (green)
  - Concept: #FF9F43 (orange)
  - Event_Topic: #FF6B6B (red)
  - Event: #54A0FF (light blue)
  - Narrative: #FF9FF3 (pink)
  - Contradiction: #FF4757 (red)
  - Healthy: #2ED573 (green)
  - Warning: #FFA502 (yellow)
  - Error: #FF4757 (red)

### Responsive Breakpoints

- Desktop: ≥ 1280px (full layout with sidebar)
- Tablet: 768-1279px (collapsed sidebar, stacked panels)
- Mobile: < 768px (bottom navigation, single column)

### Accessibility

- All interactive elements keyboard-navigable
- Focus indicators visible
- Color not the only indicator (use icons + text)
- ARIA labels on all controls
- Reduced motion support (disable animations)
- High contrast mode support
- Screen reader announcements for real-time updates
- Minimum contrast ratio: 4.5:1 for text

### Real-Time Updates

WebSocket connection management:

```typescript
interface WebSocketState {
  connected: boolean;
  reconnectAttempts: number;
  lastPong: number;
}

// On connect:
- Subscribe to channels: events, broadcasts, system
- Process incoming events:
  - events: append to feed, update event/narrative lists, update graph
  - broadcasts: update broadcast list, notify if new
  - system: update health indicator, show system alerts

// On disconnect:
- Show "Reconnecting..." banner
- Exponential backoff reconnection (1s, 2s, 4s, 8s, max 30s)
- On reconnect: fetch latest state, fill gaps

// Heartbeat:
- Client sends { type: "ping" } every 30s
- Server sends { type: "pong" } in response
- If no pong for 60s, connection is considered dead
```

## Interfaces

- `docs/api/internal-api.md` — REST and WebSocket endpoints consumed by dashboard
- `ux-principles.md` — UX principles guiding dashboard design
- `docs/architecture/service-boundaries.md` — API Gateway that serves dashboard

## Failure Modes

| Failure | Impact | Mitigation |
|--------|--------|------------|
| WebSocket disconnect | Stale data | Auto-reconnect with state reconciliation |
| API rate limit | Slow dashboard | Client-side caching; optimistic updates |
| Graph too large to render | Browser freeze | Max nodes; clustering; WebGL rendering |
| API version mismatch | Broken features | Version check at startup; graceful degradation |
| Slow API response | Loading states | Skeleton loading; stale-while-revalidate |
| Offline (no core) | Blank dashboard | Clear "Objective is not running" message |
| Large broadcast text | Slow render | Virtual scrolling; lazy rendering |

## Future Extensions

- Mobile app (React Native)
- Custom dashboard themes
- Dashboard plugins (custom widgets)
- Multi-user dashboard with auth
- Desktop app (Electron/Tauri) with local-first focus
- PWA support for offline dashboard
- Screen reader optimizations for audio dashboard
- Dashboard embed mode (iframes for external use)
- Custom dashboard layouts (drag-and-drop widgets)
- Dashboard notifications (system-level, not browser)
