# Source Types

## Purpose

Document every source category supported by Objective's ingestion system, including their data models, metadata schemas, and processing flows.

## Scope

This document covers all source adapters shipped with Objective. Custom source adapters are documented via the plugin API.

## Responsibilities

- Specify the data model for each source type
- Define source-specific metadata schemas
- Document processing flow (fetch, parse, normalize)
- Specify configuration requirements per source type
- Document limitations and known issues per source type

## Assumptions

- Source adapters are selected by `source_type` in source configuration
- Some source types require API keys or authentication
- Source availability varies; adapters handle source-specific errors
- All adapters produce the normalized RawDocument format defined in `ingestion-architecture.md`

## Design

### Source Type Registry

| Source Type | Category | Authentication | Poll Interval | Priority |
|-------------|----------|---------------|---------------|----------|
| `rss` | News | None | 30-60 min | High |
| `reddit` | Social | API Key | 15 min | Medium |
| `youtube` | Video | API Key | 60 min | Medium |
| `podcast` | Audio | None | 60-240 min | Medium |
| `pdf` | Document | None (filesystem) | On-demand | Low |
| `blog` | Web | None | 60-240 min | Medium |
| `sec_edgar` | Regulatory | Rate-limited | 1440 min | High |
| `govt_feed` | Government | Varies | 60-1440 min | High |
| `web_page` | Web | None | On-demand | Low |
| `email` | Communication | IMAP | 15 min | Medium |
| `telegram` | Social | Bot Token | 5 min | Low |
| `discord` | Social | Bot Token | 15 min | Low |
| `github` | Development | API Key | 60 min | Low |
| `arxiv` | Academic | None | 1440 min | Low |
| `hackernews` | Social | None | 15 min | Medium |

---

### RSS Source Type

**Source type identifier:** `rss`

**Description:** Fetches articles from RSS/Atom feeds. The most common ingestion source for news websites, blogs, and publications.

**Configuration:**
```yaml
- name: "reuters_world"
  type: "rss"
  url: "https://www.reuters.com/world/feed"
  poll_interval: 30
  http_headers:
    User-Agent: "Objective/1.0 (intelligence system)"
    Accept: "application/rss+xml, application/atom+xml, application/xml"
```

**Data model:**
```json
{
  "source_type": "rss",
  "external_id": "https://www.reuters.com/world/article-123",
  "url": "https://www.reuters.com/world/article-123",
  "title": "Article Title",
  "body": "Full article text extracted from HTML...",
  "body_format": "html",
  "author": "Author Name",
  "published_at": "2026-06-02T11:00:00Z",
  "metadata": {
    "feed_title": "Reuters World News",
    "feed_url": "https://www.reuters.com/world/feed",
    "categories": ["World", "Politics"],
    "image_url": "https://www.reuters.com/images/article-123.jpg",
    "description": "Article summary/excerpt"
  }
}
```

**Processing flow:**
1. Fetch URL → validate content-type is XML
2. Parse RSS/Atom XML
3. For each `<item>` or `<entry>` element:
   a. Extract title, link, description, author, pubDate, categories
   b. Fetch full article body (follow link, extract with readability algorithm)
   c. Extract main content using article extraction (readability, boilerpipe, or similar)
   d. Strip HTML, normalize whitespace
   e. Generate RawDocument
4. Store cursor: pubDate of most recent item

**Known issues:**
- Some feeds only provide summaries, not full text → requires separate article fetch
- Feed may include items already fetched → deduplication by URL and content hash
- Some feeds are large (1000+ items on first fetch) → paginate by date

---

### Reddit Source Type

**Source type identifier:** `reddit`

**Description:** Fetches posts and comments from Reddit subreddits.

**Authentication:** Reddit API requires OAuth2 credentials.

**Configuration:**
```yaml
- name: "r_worldnews"
  type: "reddit"
  subreddit: "worldnews"
  poll_interval: 15
  include_comments: true
  comment_limit: 50
  sort: "hot"                  # hot, new, top, rising
  credentials:
    client_id: "..."
    client_secret: "..."       # Encrypted at rest
```

**Data model:**
```json
{
  "source_type": "reddit",
  "external_id": "t3_abc123",
  "url": "https://www.reddit.com/r/worldnews/comments/abc123/",
  "title": "Post Title",
  "body": "Post self-text or URL content...",
  "body_format": "markdown",
  "author": "username",
  "published_at": "2026-06-02T10:30:00Z",
  "metadata": {
    "subreddit": "worldnews",
    "score": 15234,
    "upvote_ratio": 0.89,
    "num_comments": 423,
    "post_type": "link",        # link, self, image, video
    "domain": "reuters.com",
    "flair": "Breaking",
    "comments": [
      {
        "id": "t1_xyz789",
        "author": "commenter",
        "body": "Comment text...",
        "score": 234,
        "created_utc": 1685711234,
        "parent_id": "t3_abc123"
      }
    ]
  }
}
```

**Processing flow:**
1. Authenticate with OAuth2 (refresh token on expiry)
2. Fetch posts from `/r/{subreddit}/{sort}.json`
3. For each post:
   a. If `is_self`: extract selftext as body
   b. If link post: fetch linked article, extract body
   c. If `include_comments`: fetch comments thread
   d. Generate RawDocument (body = post text + top comments)
4. Update cursor: timestamp of last fetched post

**Known issues:**
- Reddit API rate limit: 60 requests/minute
- Comments can be very large → limit to top 50 by score
- Deleted posts are still returned → check `removed_by_category`

---

### YouTube Source Type

**Source type identifier:** `youtube`

**Description:** Fetches video transcripts and metadata from YouTube channels.

**Authentication:** YouTube Data API v3 key required.

**Configuration:**
```yaml
- name: "tech_youtube"
  type: "youtube"
  channel_id: "UC_x5XG1OV2P6uZZ5FSM9Ttw"
  poll_interval: 60
  include_transcript: true
  max_videos_per_poll: 10
  credentials:
    api_key: "..."             # Encrypted at rest
```

**Data model:**
```json
{
  "source_type": "youtube",
  "external_id": "abc123def45",
  "url": "https://www.youtube.com/watch?v=abc123def45",
  "title": "Video Title",
  "body": "Full video transcript text...",
  "body_format": "plaintext",
  "author": "Channel Name",
  "published_at": "2026-06-02T09:00:00Z",
  "metadata": {
    "channel_id": "UC_x5XG1OV2P6uZZ5FSM9Ttw",
    "channel_title": "Channel Name",
    "duration_seconds": 1234,
    "view_count": 150000,
    "like_count": 12000,
    "category": "Science & Technology",
    "tags": ["AI", "Machine Learning"],
    "description": "Video description text...",
    "language": "en"
  }
}
```

**Processing flow:**
1. Fetch channel's upload playlist via YouTube Data API
2. For new videos (since last cursor):
   a. Fetch video metadata (title, description, tags, stats)
   b. Fetch captions/transcript (prefer auto-generated, fallback to manual)
   c. If no captions available, skip (or flag for audio processing)
   d. Generate RawDocument
3. Update cursor: video publish date

**Known issues:**
- Transcript may not be available for all videos
- Auto-generated captions have lower accuracy
- API quota: 10,000 units/day (1 search = 100 units)
- Video descriptions may contain affiliate links and noise

---

### Podcast Source Type

**Source type identifier:** `podcast`

**Description:** Fetches podcast episodes and transcripts from RSS feeds.

**Configuration:**
```yaml
- name: "daily_news_podcast"
  type: "podcast"
  feed_url: "https://feeds.example.com/daily-news"
  poll_interval: 240
  download_audio: false        # Download MP3 for local processing
  transcribe: true             # Generate transcript via local ASR
```

**Data model:**
```json
{
  "source_type": "podcast",
  "external_id": "guid-12345",
  "url": "https://example.com/episodes/episode-123",
  "title": "Episode Title",
  "body": "Episode transcript (from ASR if downloaded)...",
  "body_format": "plaintext",
  "author": "Podcast Host Name",
  "published_at": "2026-06-02T08:00:00Z",
  "metadata": {
    "podcast_title": "Daily News Podcast",
    "feed_url": "https://feeds.example.com/daily-news",
    "duration_seconds": 3600,
    "episode_number": 123,
    "season_number": 5,
    "audio_url": "https://example.com/episode-123.mp3",
    "audio_format": "audio/mpeg",
    "audio_size_bytes": 57600000,
    "show_notes": "Episode show notes text..."
  }
}
```

**Processing flow:**
1. Fetch podcast RSS feed
2. For new episodes (since last cursor):
   a. Extract metadata (title, description, audio URL, duration)
   b. If `download_audio: true`:
      - Download audio file
      - Run local ASR (whisper.cpp) to generate transcript
      - Store transcript as body
   c. If `download_audio: false`:
      - Use description/show notes as body
      - Mark as low-confidence extraction (no transcript)
   d. If `transcribe: true` with existing audio: transcribe anyway
   e. Generate RawDocument
3. Update cursor: episode publish date

**Known issues:**
- Large audio files (60MB/hour) require significant storage
- ASR processing is CPU-intensive
- Show notes may be minimal or empty
- Some podcasts have exclusive episodes behind authentication

---

### PDF Source Type

**Source type identifier:** `pdf`

**Description:** Ingests PDF documents from local filesystem or URLs.

**Configuration:**
```yaml
- name: "research_papers"
  type: "pdf"
  paths:
    - "/Users/user/Documents/Research/"
    - "https://arxiv.org/pdf/2301.12345.pdf"
  poll_interval: 60
  recursive: true
  file_pattern: "*.pdf"
```

**Data model:**
```json
{
  "source_type": "pdf",
  "external_id": "sha256-of-content",
  "url": "file:///Users/user/Documents/Research/paper.pdf",
  "title": "Extracted Title",
  "body": "Full text extracted via OCR or text extraction...",
  "body_format": "markdown",
  "author": "Extracted Authors",
  "published_at": "2026-06-01T00:00:00Z",
  "metadata": {
    "filename": "paper.pdf",
    "path": "/Users/user/Documents/Research/paper.pdf",
    "file_size_bytes": 2450000,
    "page_count": 12,
    "pdf_version": "1.7",
    "has_ocr": true,
    "extraction_method": "pdftotext",
    "extraction_confidence": 0.92
  }
}
```

**Processing flow:**
1. If filesystem source: watch directory for new/modified files
2. If URL source: download PDF
3. Validate PDF format
4. Extract text:
   a. Try direct text extraction (pdftotext, lopdf)
   b. If result is < 50% of expected content, fall back to OCR (tesseract)
5. Extract metadata (title, author, subject via PDF info)
6. Generate RawDocument
7. Track files by content hash and modification time

**Known issues:**
- Scanned PDFs require OCR (slow, less accurate)
- PDF text extraction quality varies by PDF generator
- Some PDFs are image-only → OCR required
- Large PDFs (>100 pages) may timeout → process first N pages

---

### Blog Source Type

**Source type identifier:** `blog`

**Description:** Fetches blog posts from websites without RSS feeds, using sitemaps or URL pattern scraping.

**Configuration:**
```yaml
- name: "tech_blog"
  type: "blog"
  url: "https://blog.example.com"
  sitemap: "https://blog.example.com/sitemap.xml"
  poll_interval: 240
  article_selector: "article"  # CSS selector for main content
```

**Data model:** Same as RSS (body is full article text).

**Processing flow:**
1. Fetch sitemap.xml to discover article URLs
2. For new URLs (not previously fetched):
   a. Fetch article HTML
   b. Extract main content using readability algorithm
   c. Extract metadata (author, date, categories from meta tags)
   d. Generate RawDocument
3. Fallback: if no sitemap, scrape index page for article links

**Known issues:**
- No standard article structure → content extraction may fail
- Sitemap may not include all articles
- JavaScript-rendered sites not supported (no headless browser)

---

### SEC EDGAR Source Type

**Source type identifier:** `sec_edgar`

**Description:** Fetches SEC filings (10-K, 10-Q, 8-K, etc.) from the EDGAR system.

**Configuration:**
```yaml
- name: "aapl_sec"
  type: "sec_edgar"
  ticker: "AAPL"
  filing_types: ["10-K", "10-Q", "8-K"]
  poll_interval: 1440
  include_exhibits: false
```

**Data model:**
```json
{
  "source_type": "sec_edgar",
  "external_id": "0000320193-26-000045",
  "url": "https://www.sec.gov/cgi-bin/browse-edgar?action=getcompany&CIK=AAPL",
  "title": "APPLE INC 10-K (Annual Report)",
  "body": "Full filing text extracted from HTML/XML...",
  "body_format": "html",
  "author": "APPLE INC",
  "published_at": "2026-06-01T16:00:00Z",
  "metadata": {
    "cik": "0000320193",
    "ticker": "AAPL",
    "filing_type": "10-K",
    "filing_date": "2026-06-01",
    "period_end": "2025-09-30",
    "company_name": "APPLE INC",
    " sic": "3571",
    "fiscal_year_end": "0930",
    "document_count": 12,
    "exhibits": ["EX-10.1", "EX-21.1", "EX-31.1"]
  }
}
```

**Processing flow:**
1. Respect SEC rate limits (10 requests/second)
2. Query EDGAR full-text search for company CIK
3. Fetch filing index for filing types
4. For each new filing:
   a. Fetch primary document (HTML or XBRL)
   b. Extract text content (strip HTML/XML)
   c. Extract structured data from XBRL (financial statements)
   d. Generate RawDocument
5. Update cursor: latest filing date

**Known issues:**
- SEC blocks excessive requests → must respect rate limits
- HTML filings are poorly formatted
- XBRL parsing is complex
- Filing URLs change if company restructures EDGAR presence

---

### Government Feed Source Type

**Source type identifier:** `govt_feed`

**Description:** Fetches government data feeds (GPO, Federal Register, FDA, CDC, etc.).

**Configuration:**
```yaml
- name: "federal_register"
  type: "govt_feed"
  feed_url: "https://www.federalregister.gov/api/v1/documents"
  agency: "FCC"
  poll_interval: 1440
```

**Data model:** Similar to RSS with additional government metadata.

**Known issues:**
- Each agency has a different API format
- Some feeds require FOIA requests for non-public data
- Government API stability varies

---

### Web Page Source Type

**Source type identifier:** `web_page`

**Description:** One-off fetch of a specific web page (not a feed).

**Configuration:**
```yaml
- name: "about_page"
  type: "web_page"
  url: "https://company.example.com/about"
  poll_interval: 1440          # Check daily for changes
```

**Data model:** Same as RSS.

**Processing flow:**
1. Fetch URL
2. Extract main content (readability algorithm)
3. Compute content hash for change detection
4. If content hash differs from previous, generate RawDocument

**Known issues:**
- No change detection metadata → must compare full content
- Some pages require cookies or session state
- Pages behind login walls are not supported
- Dynamic content (JavaScript-rendered) not captured

---

### Email Source Type

**Source type identifier:** `email`

**Description:** Fetches emails from IMAP mailboxes.

**Authentication:** IMAP password or OAuth2 token.

**Configuration:**
```yaml
- name: "newsletter_inbox"
  type: "email"
  imap_server: "imap.gmail.com"
  username: "user@gmail.com"
  mailbox: "INBOX"
  search_filter: "FROM newsletter@company.com"
  poll_interval: 15
  mark_as_read: true
```

**Data model:**
```json
{
  "source_type": "email",
  "external_id": "message-id@mail.gmail.com",
  "url": "imap://user@gmail.com/INBOX/12345",
  "title": "Email Subject",
  "body": "Email body text (HTML stripped)...",
  "body_format": "plaintext",
  "author": "Sender Name <sender@example.com>",
  "published_at": "2026-06-02T10:00:00Z",
  "metadata": {
    "from": "sender@example.com",
    "to": ["user@gmail.com"],
    "cc": [],
    "message_id": "<msg-id@mail.gmail.com>",
    "in_reply_to": null,
    "attachments": []
  }
}
```

**Known issues:**
- OAuth2 setup is non-trivial
- HTML emails require thorough sanitization
- Attachment extraction requires MIME parsing
- Large mailboxes require significant initial sync

---

### Telegram Source Type

**Source type identifier:** `telegram`

**Description:** Monitors Telegram channels for new messages.

**Configuration:**
```yaml
- name: "news_channel"
  type: "telegram"
  channel: "@channel_username"
  bot_token: "..."             # Encrypted at rest
  poll_interval: 5
```

**Data model:** Message text with channel metadata.

**Known issues:**
- Requires Telegram bot token
- Only public channels are accessible
- Media messages (images, video) not supported
- Bot may be rate-limited

---

### Discord Source Type

**Source type identifier:** `discord`

**Description:** Monitors Discord channels for new messages.

**Configuration:**
```yaml
- name: "community_discord"
  type: "discord"
  guild_id: "123456789"
  channel_ids: ["987654321"]
  bot_token: "..."             # Encrypted at rest
  poll_interval: 15
```

**Data model:** Message text with author and channel metadata.

**Known issues:**
- Requires Discord bot setup with appropriate permissions
- Gateway intent must be enabled for message content
- Large servers may have many channels → filter by channel

---

### GitHub Source Type

**Source type identifier:** `github`

**Description:** Monitors GitHub repositories for releases, issues, discussions.

**Authentication:** GitHub Personal Access Token.

**Configuration:**
```yaml
- name: "objective_github"
  type: "github"
  owner: "anomalyco"
  repo: "objective"
  events: ["releases", "issues", "discussions"]
  poll_interval: 60
```

**Data model:** Repository event data with full text content.

**Known issues:**
- API rate limit: 5000 requests/hour (authenticated)
- Issues and discussions can have very long threads
- Markdown content requires sanitization

---

### ArXiv Source Type

**Source type identifier:** `arxiv`

**Description:** Fetches academic papers from ArXiv.

**Configuration:**
```yaml
- name: "cs_papers"
  type: "arxiv"
  categories: ["cs.AI", "cs.LG", "cs.CL"]
  max_results: 100
  poll_interval: 1440
```

**Data model:** Paper abstract with authors, categories, PDF link.

**Known issues:**
- Only abstracts available via API; full text requires PDF download
- ArXiv rate limits: 1 request/3 seconds
- Some papers are withdrawn → check for withdrawal notices

---

### Hacker News Source Type

**Source type identifier:** `hackernews`

**Description:** Fetches top stories and comments from Hacker News.

**Configuration:**
```yaml
- name: "hn_frontpage"
  type: "hackernews"
  feed: "top"                  # top, new, best, ask, show
  poll_interval: 15
```

**Data model:** Story title, URL, points, comment text.

**Known issues:**
- Firebase API has occasional downtime
- Some stories are job postings or non-news content

## Interfaces

- `ingestion-architecture.md` — ingestion pipeline and adapter contract
- `docs/api/plugin-api.md` — custom source adapter development
- `docs/data/storage-architecture.md` — document archive storage

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Source type not registered | Configuration error | Validate source types at startup |
| API key expired | Source fails to fetch | Monitor auth errors, alert user |
| Source format changes | Parse failures | Adapter versioning, test coverage |
| Large source backlog | Resource spike | Per-source rate limiting, batch processing |
| Source deprecated | No new data | Monitor source health, recommend alternatives |

## Future Extensions

- Headless browser support for JavaScript-rendered content
- Twitter/X API integration (if API access becomes available again)
- Bluesky/AT Protocol source
- Substack source adapter
- Wikipedia recent changes feed
- Custom API source adapter (generic HTTP JSON API to document pipeline)
- RSS-to-email bridge (fetch newsletters via email ingestion)
