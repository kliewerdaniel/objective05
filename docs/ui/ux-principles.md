# UX Principles

## Purpose

Define the user experience philosophy and principles that guide all user-facing design decisions in Objective.

## Scope

This document covers UX goals, design principles, information density guidelines, discoverability strategies, accessibility requirements, and interaction patterns.

## Responsibilities

- Establish UX principles that guide design decisions
- Define information density standards
- Specify discoverability strategies
- Document accessibility requirements
- Guide tradeoff decisions in UI design

## Assumptions

- Primary users are knowledge workers, researchers, analysts, and enthusiasts
- Users are willing to invest time in learning the system for long-term value
- Information density is a feature, not a bug — power users prefer data-rich views
- The system runs continuously; the UI is a window into a running process
- Users may check the dashboard infrequently (daily) or frequently (hourly)

## Design

### UX Goals

| Goal | Description | Metric |
|------|-------------|--------|
| Transparency | Users understand what the system is doing at all times | "Why did the system report this?" answers traceable to sources |
| Trust | Users trust the system's outputs | Confidence scores match user perception; sources always visible |
| Control | Users can configure, override, and correct the system | Correction actions take < 3 clicks |
| Awareness | Users know what happened while they were away | "Since you last checked" summaries |
| Efficiency | Common tasks require minimal actions | Most dashboard tasks in 1-2 clicks |

### Design Principles

**1. Progressive Disclosure**
Show the most important information first. Provide drill-down for detail.
- Feed page: headline + 2-line summary → event detail → full provenance
- Never hide critical information (confidence, source, date)
- Use expandable sections, not nested pages, for related data

**2. Information Density**
- Default view shows as much useful information as fits without scrolling
- Use visual encoding (color, size, position) to convey additional dimensions
- Compact table views for power users; card views for casual browsing
- User can toggle between density levels (compact / normal / comfortable)

**3. Real-Time Awareness**
- Visual indicators for live/loading/stale states
- "N new items" banners (don't auto-scroll — user controls position)
- Timestamps relative ("2m ago") with absolute on hover
- Status bar always visible: system health, last update, source count

**4. Source Transparency**
- Every claim and entity links to its source document
- Confidence scores displayed prominently (color-coded: green > 0.8, yellow > 0.5, red < 0.5)
- "Why this?" tooltip explains how the system reached a conclusion
- Contradictions show both sides with source attribution

**5. User Control**
- All automatic decisions can be overridden: merge events, resolve contradictions, delete entities
- Corrections feed into the system's confidence model
- Undo for all destructive actions (with 5-second grace period)
- Export any view (CSV, JSON, Markdown)

**6. Graceful Degradation**
- If the system is processing: show progress indicators
- If a service is unavailable: show degraded state, not errors
- If models are loading: show "Indexing in progress — results may be incomplete"
- If offline: use cached data, show "Offline" indicator

**7. Discoverability**
- All features discoverable through navigation or search
- Command palette (Cmd+K) for power users
- Onboarding wizard for first-time setup
- Contextual help ("?") on complex features
- Tooltip explanations for all data fields (confidence, importance, etc.)

**8. Consistency**
- Same action (click, drag, right-click) has consistent behavior across views
- Same visual encoding (color, icon) across all pages
- Same layout patterns (filter bar above lists, detail on click)
- Same date/time format throughout

### Information Density Guidelines

| Context | Density | Elements per view | Detail Level |
|---------|---------|------------------|--------------|
| Feed page | High | 20-30 items | Headline + summary + metadata |
| Event list | Normal | 15-20 cards | Card with title, importance, date, source count |
| Graph view | Variable | 50-500 nodes | Labels on hover; detail on click |
| Broadcast reader | Comfortable | 1 broadcast | Full text with styling |
| Source list | High | All sources (table) | Name, type, status, last poll, count |
| Settings | Normal | 1 section at a time | Full configuration with descriptions |

### Discoverability Strategies

**New user onboarding:**
1. Welcome screen: "Objective is setting up..." (while models download)
2. Source setup wizard: "Add your first source" (recommended: RSS feed)
3. First broadcast generation: "Your first broadcast is ready"
4. Dashboard tour: highlights key areas (3-5 tooltip overlays)

**Ongoing discovery:**
- "Did you know?" tips in status bar (rotating, weekly)
- New feature indicators (blue dot on navigation items)
- Release notes shown after updates
- Search (Cmd+K) indexes all navigation items and recently viewed items

**Complex feature help:**
- Inline explanations for advanced features ("Why would I use this?")
- Example workflows ("Try: search for an entity → view its events → listen to related broadcast")
- Keyboard shortcut reference (accessible via "?")

### Accessibility Requirements

- All interactive elements must be keyboard-accessible
- Focus indicators: 2px solid ring, minimum 3:1 contrast
- Screen reader support: ARIA labels on all dynamic content
- Color contrast: minimum 4.5:1 for text, 3:1 for large text
- Color is never the sole indicator of state (add icons, text, patterns)
- Reduced motion: disable animations when `prefers-reduced-motion` is set
- Text size: minimum 14px body text; support browser zoom to 200%
- Touch targets: minimum 44x44px for interactive elements
- Focus order: logical, follows visual layout

### Interaction Patterns

**Common interactions:**
| Pattern | Behavior |
|---------|----------|
| Click item | Navigate to detail (same page slide-in for lists) |
| Right-click | Context menu (mark as read, exclude, flag) |
| Drag | Reorder (list views), connect (graph view) |
| Hover | Tooltip with summary, confidence, source |
| Scroll | Load more (infinite scroll on lists) |
| Cmd+K | Command palette |
| Esc | Close panel, cancel action |
| Ctrl+Z | Undo last action |

**Confirmation patterns:**
| Action | Confirmation |
|--------|-------------|
| Delete source | "Are you sure? This will stop fetching from this source." + Undo button |
| Resolve contradiction | "Resolve in favor of which claim?" + select + confirm |
| Merge entities | "Merge Entity A into Entity B?" + preview of result |
| Clear data | Type "DELETE" to confirm |

**Notification patterns:**
| Notification | Type | Display |
|-------------|------|---------|
| Breaking news | Urgent | Top banner + audio alert (optional) |
| Broadcast ready | Info | Toast notification (bottom-right) |
| Contradiction detected | Warning | Yellow badge on contradiction nav item |
| Source error | Error | Red badge on sources nav item |
| System update | Info | Toast notification |
| Model download complete | Info | Toast notification |

## Interfaces

- `dashboard-spec.md` — UI specification guided by these principles
- `docs/vision/vision.md` — product philosophy driving UX decisions

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Feature buried too deep | Low usage | Analytics on feature usage; surface unused features |
| Information overload | User confusion | Progressive disclosure; customizable density |
| Accessibility gaps | Exclusion | Automated accessibility testing in CI; manual audit quarterly |
| Inconsistency across views | User confusion | UI component library with documented patterns |
| Slow interactions | Frustration | Loading states; optimistic updates; stale-while-revalidate |
| Too many notifications | Alert fatigue | Configurable notification preferences; grouping |

## Future Extensions

- Personalization engine (adaptive UI based on usage patterns)
- Multi-user UX with profiles and permissions
- Collaborative features (share views, annotations)
- Mobile-responsive dashboard
- Voice-controlled dashboard (accessibility)
- Dashboard as a screensaver/kiosk mode
- Custom CSS/theming support
- Plugin-powered dashboard widgets
