# Objective Dashboard — Design System v1

> Category: Intelligence Dashboard
> Dark-themed monitoring dashboard with glassmorphism, cyber-violet accents, and data-dense layouts for real-time intelligence analysis.

## 1. Visual Theme & Atmosphere

Dark cyber-aesthetic with glass-like panels, gradient accents, and subtle glow effects. Deep navy-slate backgrounds (`#0a0b10`) layered with translucent glass cards (`backdrop-filter: blur(12px)`) create depth. The violet-to-teal gradient (`#8b5cf6` → `#06b6d4`) serves as the primary accent signal across interactive elements, active states, and brand typography.

**Key Characteristics:**
- Dark theme with `color-scheme: dark`
- Glassmorphism: semi-transparent panels with backdrop blur
- Cyber-violet (`#8b5cf6`) as primary accent, neon-teal (`#06b6d4`) as secondary
- Gradient text effects on display headings
- Glow shadows on hover: `0 0 20px rgba(139, 92, 246, 0.15)`
- Subtle radial gradient overlay on main content: `radial-gradient(circle at 50% 0%, rgba(139, 92, 246, 0.05) 0%, transparent 50%)`

## 2. Color

### Primary Background
- **`--bg-primary`**: `#0a0b10` — Page body, scrollbar track
- **`--bg-secondary`**: `#11131c` — Sidebar, Header, Footer, card base
- **`--bg-tertiary`**: `#181b28` — Inputs, secondary buttons, progress bars, toggle off
- **`--bg-glass`**: `rgba(17, 19, 28, 0.7)` — Glass card/panel backgrounds
- **`--bg-glass-hover`**: `rgba(24, 27, 40, 0.85)` — Hover state for glass surfaces

### Text
- **`--text-primary`**: `#f8fafc` — Headings, titles, active states
- **`--text-secondary`**: `#94a3b8` — Body copy, nav defaults, labels
- **`--text-muted`**: `#64748b` — Timestamps, meta, placeholders

### Accent
- **`--accent-primary`**: `#8b5cf6` (Cyber Violet) — Active icons, nav indicator, badges, focus rings
- **`--accent-secondary`**: `#06b6d4` (Neon Teal) — Secondary badges, importance low fill
- **`--accent-gradient`**: `linear-gradient(135deg, #8b5cf6 0%, #06b6d4 100%)` — Logo, primary buttons, active pills, progress bars, title display

### Semantic
- **Success `#10b981`**, **Warning `#f59e0b`**, **Danger `#ef4444`**, **Info `#3b82f6`**
- Status dots with glow: `0 0 8px <color>`
- Semantic backgrounds: `rgba(<color>, 0.1)` with `1px solid rgba(<color>, 0.2)` borders

### Borders
- **`--border-color`**: `rgba(255, 255, 255, 0.08)` — Default borders
- **`--border-color-hover`**: `rgba(255, 255, 255, 0.15)` — Hover borders
- **Inline dividers**: `rgba(255, 255, 255, 0.03)` to `0.04`

## 3. Typography

### Font Families
- **Display**: `'Outfit', system-ui, -apple-system, sans-serif` — Headings, titles, logo, metric values
- **Body**: `'Plus Jakarta Sans', system-ui, -apple-system, sans-serif` — Body text, inputs, buttons, UI
- **Mono**: `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace` — Timestamps, URLs, code values

### Scale & Hierarchy
- **Hero/Display Title**: `--font-display`, weight 700, `-0.03em` tracking, accent gradient clip
- **Heading Large (h1)**: 1.75rem (28px), weight 600, `-0.02em` tracking
- **Heading Medium (h2/h3)**: 1.25rem (20px), weight 600
- **Header Title**: 1.5rem (24px), weight 700
- **Section Title**: 0.75rem (12px), weight 700, uppercase, `0.05em` tracking
- **Body**: 0.9rem (14.4px), line-height 1.5
- **Small/Meta**: 0.75rem (12px)
- **Badge/Section label**: 0.75rem, weight 600, uppercase, `0.05em` tracking
- **Navigation**: 0.9rem (14.4px), weight 500
- **Monospace**: Timestamps, source URLs, config values

## 4. Spacing & Grid

### Scale (rem-based, ~16px base)
- Fine: 0.25rem (4px), 0.35rem (~5.6px), 0.4rem (~6.4px), 0.5rem (8px)
- Standard: 0.75rem (12px), 0.85rem (~13.6px), 1rem (16px)
- Large: 1.25rem (20px), 1.5rem (24px), 1.75rem (28px), 2rem (32px)
- Extended: 4rem (64px), 6rem (96px)

### Component Padding Patterns
- **Cards**: 1.5rem all sides
- **Glass panels**: 2rem all sides
- **Inputs**: 0.75rem vertical, 1rem horizontal
- **Primary buttons**: 0.6rem vertical, 1.25rem horizontal
- **Pill buttons**: 0.4rem vertical, 1rem horizontal
- **Badges**: 0.25rem vertical, 0.75rem horizontal

### Gap Patterns
- **Card content**: 0.75–0.85rem
- **Grid items**: 1.25rem
- **Section containers**: 1.5rem
- **Form fields**: 1.25rem
- **Navigation items**: 1rem
- **Sidebar sections**: 0.25rem

## 5. Layout & Composition

### Primary Layout
```
[Sidebar 260px] [Main: Header + Content + Footer]
```
- Sidebar: `position: sticky; top: 0; height: 100vh; z-index: 50`
- Header: `z-index: 40`
- Footer: `z-index: 30`
- Detail drawer overlay: `z-index: 100`

### Content Max-Width: 1400px, centered

### Page Grid Systems
- **Events**: `repeat(auto-fill, minmax(360px, 1fr))`
- **Narratives**: `1fr 1fr` (2 columns)
- **Sources**: `repeat(auto-fill, minmax(280px, 1fr))`
- **Broadcasts**: `380px 1fr` (list + viewer)
- **Settings**: `240px 1fr` (nav + content)
- **Graph**: `1fr auto` (canvas + detail panel)
- **Telemetry**: `repeat(4, 1fr)`

### Component Padding
- Content body: 2rem (1rem on mobile)
- Sidebar: 1rem horizontal padding
- Header: 2rem horizontal padding

## 6. Components

### Glass Card (Base)
- Background: `rgba(17, 19, 28, 0.7)`, backdrop-filter: `blur(12px)`
- Border: `1px solid rgba(255, 255, 255, 0.08)`
- Border-radius: 12px
- Padding: 1.5rem
- Shadow: `0 4px 6px -1px rgba(0,0,0,0.1), 0 2px 4px -1px rgba(0,0,0,0.06)`
- Hover: border lightens, `translateY(-2px)`, shadow-lg + glow

### Glass Panel
- Same glass background/blur/border
- Border-radius: 18px, padding: 2rem, shadow-lg

### Buttons
- **Primary**: Gradient background (`#8b5cf6` → `#06b6d4`), 12px radius, `0 4px 12px rgba(139,92,246,0.3)` shadow, hover `translateY(-1px)`
- **Secondary**: Dark fill (`#181b28`), 1px border, 12px radius
- **Pill filter**: 9999px radius, gradient active state, 0.85rem text
- **Play/Icon**: 44px circle, gradient fill, glow shadow, hover scale(1.05)

### Form Inputs
- Background: `#181b28`, 12px radius, 1px border
- Focus: violet border + `0 0 0 3px rgba(139,92,246,0.2)` ring
- Search: padded-left for icon

### Toggle Switch
- 44×22px track, 16px circular knob
- Off: tertiary background; On: violet background
- Knob slides 22px on toggle

### Badges
- 9999px pill shape, uppercase, 0.75rem, weight 600
- Colored background (10% opacity) + border (20% opacity)

### Navigation (Sidebar)
- Items: 0.9rem, weight 500, 12px radius, gap with icon
- Active: violet-tinted background (`rgba(139,92,246,0.08)`), violet icon, 3px left indicator
- Hover: subtle white overlay (`rgba(255,255,255,0.03)`)

### Detail Drawer
- 500px right sheet, 100vh height, `-10px 0 30px rgba(0,0,0,0.2)` shadow
- `rgba(0,0,0,0.4)` overlay with `blur(4px)`
- Slide-in animation: `0.25s cubic-bezier(0.16, 1, 0.3, 1)`

### Progress Bars
- 4–5px height, 9999px radius, `rgba(255,255,255,0.05)` track
- Fill colors: danger (≥0.75), warning (≥0.5), teal (<0.5) or violet
- Gradient fill for audio progress

### Entity Tags
- 4–6px radius, tertiary background, 1px border, 0.8rem
- Hover: violet border accent

## 7. Motion & Interaction

### Transitions
- Fast: `0.15s ease` — hovers, focuses, active states
- Normal: `0.25s ease` — layout shifts, card transformations

### Animations
- **fadeIn**: `0.3s ease` (opacity + translateY 4px) — page entries, toasts
- **slideInSheet**: `0.25s cubic-bezier(0.16, 1, 0.3, 1)` — drawer open
- **spin**: `1s linear infinite` — loading indicator
- **pulse-slow**: `2s infinite ease-in-out` — connection status indicator
- **slideUp**: `0.3s cubic-bezier(0.16, 1, 0.3, 1)` — toast notifications

### Hover Effects
- Cards: lift `-2px`, enhance border, add glow shadow
- Buttons: lift `-1px` or scale `1.05`
- Interactive elements: border color shift
- Briefing card chevron: `translateX(3px)`

## 8. Voice & Brand

### Personality
Professional, technical, authoritative. The dashboard communicates system health and intelligence data with clarity and precision. "Mission control" tone — concise, data-forward, trustworthy.

### Language Patterns
- Interface labels are literal and functional
- Status indicators use clear vocabulary: Running, Paused, Healthy, Degraded
- Empty states are informative, not apologetic
- Action labels are imperative and specific

### Visual Brand
- Violet/teal gradient as signature visual
- Glassmorphism as core surface treatment
- Outfit font for display hierarchy
- Subtle glow effects for interactive signals

## 9. Anti-patterns

- Do not use pure black (`#000000`) — always use `#0a0b10` or warmer dark tones
- Do not mix glassmorphism with flat surfaces in the same context
- Do not use gradient text for body copy or interactive labels (accessibility)
- Do not exceed 12px border-radius on interactive elements
- Do not use the violet accent on non-interactive decorative elements
- Do not stack multiple glow effects on the same element
- Do not use the display font (`Outfit`) for body text or UI labels
- Do not introduce new accent colors beyond the violet/teal gradient
- Do not place critical information in tooltips or hover-only states
- Do not use backdrop-filter on elements that contain scrolling content (performance)
