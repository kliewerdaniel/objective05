# Audio System

## Purpose

Define the audio system — the subsystem responsible for converting broadcast text into natural speech, managing voices, generating podcast-style audio, and serving audio content for consumption.

## Scope

This document covers text-to-speech architecture, voice management, podcast generation, audio archiving, streaming, and audio storage.

## Responsibilities

- Convert broadcast text to natural-sounding speech
- Manage multiple voices for podcast-style content
- Generate podcast episodes with intro/outro/transitions
- Archive generated audio for playback
- Serve audio for streaming and download
- Support playback progress tracking

## Assumptions

- TTS runs locally (no cloud TTS API dependency)
- Audio quality is important but real-time is not (generation is async)
- Multiple voices are desired for listenability
- Storage for audio is significant (see storage estimates)
- Users may want to export audio for mobile listening

## Design

### Audio Pipeline

```
Broadcast Text (from Broadcast Engine)
    │
    ▼
┌──────────────────┐
│  Script Parser    │  Split text into speaker segments
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Voice Selector   │  Assign voices to segments
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  TTS Engine       │  Generate audio per segment (parallel)
│  (piper or        │
│   Coqui or Oute)  │
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Audio Assembly   │  Mix segments + intro/outro/transitions
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Post-Processing  │  Normalize volume, add silence padding
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Encoding         │  Encode MP3/OGG at configured bitrate
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Storage & Index  │  Archive with metadata
└──────────────────┘
```

### TTS Architecture

**Runtime selection:**

| TTS Engine | Quality | Speed | Language Support | Model Size |
|-----------|---------|-------|-----------------|------------|
| Piper TTS | Good | Fast | 20+ languages | ~100MB/voice |
| Coqui TTS | Very Good | Moderate | 10+ languages | ~300MB/voice |
| OuteTTS | Excellent | Slow | English only | ~1GB/voice |

**Recommendation:** Use Piper TTS as default (fast, small, multi-lingual). Optionally upgrade to Coqui or OuteTTS for higher quality.

```yaml
tts:
  engine: "piper"
  model: "en_US-lessac-medium"     # Default voice
  sample_rate: 22050               # Hz
  audio_format: "mp3"
  bitrate: "128k"                  # kbps
  multi_voice: true                # Use different voices for variety
  voice_pool:
    - "en_US-lessac-medium"        # Main anchor
    - "en_US-amy-medium"           # Co-anchor
    - "en_US-norman-medium"        # Correspondent
```

### Script Parsing

Broadcast text is parsed into segments with speaker assignments:

```rust
pub struct AudioScript {
    pub segments: Vec<AudioSegment>,
    pub metadata: AudioMetadata,
}

pub struct AudioSegment {
    pub speaker_id: String,
    pub text: String,
    pub segment_type: SegmentType,
    pub estimated_duration_seconds: f32,
}

pub enum SegmentType {
    Intro,
    TopStory,
    NarrativeUpdate,
    ContradictionAlert,
    DeepDive,
    Transition,
    Outro,
    IdleContent,
}
```

**Script format conventions:**
```
[ANCHOR]: Welcome to Objective Intelligence Briefing for June 2, 2026.
[ANCHOR]: Our top story today...

[CORRESPONDENT]: Reporting from Washington, here are the latest developments...

[ANCHOR]: In other news...

[ANCHOR]: That concludes today's briefing. We'll be back with updates in 2 hours.
```

**Parsing rules:**
1. Lines starting with `[SPEAKER]:` are assigned to that speaker
2. Lines without speaker prefix use the default (last speaker)
3. Empty lines are treated as segment boundaries
4. Special markers `[INTRO]`, `[OUTRO]` override segment type
5. Unrecognized speakers are logged and assigned to anchor fallback

### Voice Management

```rust
pub struct VoiceManager {
    pub voices: Vec<Voice>,
    pub assignment_strategy: VoiceAssignment,
}

pub struct Voice {
    pub id: String,              // "en_US-lessac-medium"
    pub name: String,            // "James"
    pub gender: Option<String>,  // "male", "female"
    pub language: String,        // "en-US"
    pub style: VoiceStyle,       // anchor, correspondent, narrator
    pub model_path: String,      // Filesystem path to TTS model
    pub config_path: String,     // Filesystem path to config JSON
}

pub enum VoiceStyle {
    Anchor,         // Primary host — clear, authoritative
    Correspondent,  // Reporter — energetic, engaged
    Narrator,       // Deep dive — calm, measured
    Analyst,        // Analysis — thoughtful, deliberate
}
```

**Voice assignment:**
- Anchor: main segments (top story, intro, outro)
- Correspondent: breaking news, field reports
- Narrator: deep dives, background context
- Analyst: contradictions, analysis segments
- Rotate anchor voice per broadcast (variety)

**Voice pool fallback:**
If a requested voice is unavailable:
1. Try another voice with same style
2. Try any available voice
3. Use default voice (anchor)

### Podcast Generation

Podcast-style audio adds production value:

```rust
pub struct PodcastConfig {
    pub intro: AudioAsset,         // Opening music/hook
    pub outro: AudioAsset,         // Closing music
    pub transitions: Vec<AudioAsset>, // Between-segment transitions
    pub crossfade_duration_ms: u32,   // Default: 500
    pub silence_between_segments_ms: u32, // Default: 1000
    pub normalize_volume: bool,    // Default: true
    pub target_loudness_lufs: f32, // Default: -16
}
```

**Audio assets:**
```
~/.objective/audio/library/
├── intro.mp3                    # Opening (5-15 seconds)
├── outro.mp3                    # Closing (5-15 seconds)
├── transitions/
│   ├── transition-1.mp3         # Between segments (2-3 seconds)
│   └── transition-2.mp3
└── stings/
    ├── breaking-news.mp3        # Breaking news alert
    └── contradiction.mp3        # Contradiction alert
```

**Generation steps:**
1. Generate intro: Play intro music (fade in) → Anchor greeting
2. For each segment:
   a. Play transition music (if not first segment)
   b. Generate TTS for segment text
   c. Apply crossfade if adjacent to music
3. Generate outro: Anchor sign-off → Play outro music (fade out)
4. Post-processing:
   a. Normalize volume to target loudness (EBU R128)
   b. Strip silence from beginning and end
   c. Encode to MP3 at configured bitrate
5. Write audio file with metadata tags (title, date, episode number)

### Audio Encoding

```rust
pub struct EncodingConfig {
    pub format: AudioFormat,        // MP3, OGG, WAV, FLAC
    pub bitrate: String,            // "128k", "192k", "256k"
    pub sample_rate: u32,           // 22050, 44100, 48000
    pub channels: u8,               // 1 (mono), 2 (stereo)
    pub quality: u8,                // 0-10 (for VBR)
}
```

**Recommendations:**
- Spoken word (podcast): MP3 128kbps, 22050Hz, mono
- Music (intro/outro): MP3 192kbps, 44100Hz, stereo
- Archive: FLAC for lossless (if storage allows)

### Audio Archive

```rust
pub struct AudioArchive {
    pub broadcasts: Vec<ArchivedBroadcast>,
    pub retention_days: u32,           // Default: 90
    pub auto_export: Option<String>,   // Directory to copy files
}

pub struct ArchivedBroadcast {
    pub id: String,
    pub title: String,
    pub generated_at: DateTime<Utc>,
    pub duration_seconds: u32,
    pub file_path: PathBuf,
    pub format: AudioFormat,
    pub file_size_bytes: u64,
    pub metadata: AudioMetadata,
}
```

**Directory layout:**
```
~/.objective/audio/broadcasts/
├── 2026-06-02/
│   ├── morning-brief.mp3
│   ├── morning-brief.json          # Metadata
│   ├── breaking-us-election.mp3
│   └── evening-brief.mp3
├── 2026-06-03/
│   └── ...
└── library/
    ├── intro.mp3
    └── ...
```

**Metadata file (sidecar JSON):**

```json
{
  "id": "bcast-20260602-0600",
  "title": "Morning Briefing - June 2, 2026",
  "generated_at": "2026-06-02T06:00:00Z",
  "duration_seconds": 1245,
  "format": "mp3",
  "bitrate": "128k",
  "sample_rate": 22050,
  "channels": 1,
  "segments": [
    {"type": "intro", "speaker": "anchor", "duration_seconds": 15},
    {"type": "top_story", "speaker": "anchor", "duration_seconds": 180},
    {"type": "transition", "duration_seconds": 3},
    {"type": "narrative_update", "speaker": "correspondent", "duration_seconds": 240},
    {"type": "outro", "speaker": "anchor", "duration_seconds": 10}
  ],
  "word_count": 1850,
  "content_hash": "sha256:abc123..."
}
```

### Streaming

Audio broadcasts are available for streaming via the API:

```rust
pub struct StreamingEndpoint {
    pub enabled: bool,
    pub port: u16,                    // Default: 8081
    pub max_bitrate: String,          // "128k"
    pub buffer_size: usize,           // 16KB
    pub cors_allowed_origins: Vec<String>,
}
```

**Endpoints:**

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/audio/broadcasts` | List broadcasts (paginated) |
| GET | `/api/audio/broadcasts/{id}` | Stream audio file |
| GET | `/api/audio/broadcasts/{id}/metadata` | Get metadata JSON |
| GET | `/api/audio/latest` | Stream most recent broadcast |
| GET | `/api/audio/playlist.m3u` | HLS playlist |
| DELETE | `/api/audio/broadcasts/{id}` | Delete broadcast |

**Playback tracking:**
- API tracks playback progress (optional, opt-in)
- "Resume from last position" available in UI
- Playback history stored in knowledge graph

### Audio Maintenance

```rust
pub struct AudioMaintenance {
    pub retention_days: u32,           // Default: 90
    pub max_storage_gb: u32,           // Default: 10
    pub cleanup_interval_hours: u32,   // Default: 24
    pub archive_command: Option<String>, // External archiving command
}
```

**Maintenance tasks (daily):**
1. Delete broadcasts older than retention_days
2. If total audio storage > max_storage_gb, delete oldest broadcasts
3. Run optional archive_command for external backup
4. Rebuild audio index (in case of file system changes)
5. Verify audio file integrity (check headers, duration)

## Interfaces

- `broadcast-engine.md` — upstream text provider
- `docs/data/storage-architecture.md` — audio storage layout
- `docs/api/internal-api.md` — audio streaming endpoints
- `docs/ui/dashboard-spec.md` — audio player in dashboard

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| TTS model not found | Audio generation fails | Log clear error with model download instructions; generate text-only broadcast |
| TTS inference too slow | Audio delayed | Generate in background; deliver text first, audio when ready |
| Audio file corruption | Playback fails | Verify audio output; regenerate on failure |
| Storage full for audio | New audio fails | Prune oldest broadcasts first; alert user |
| Voice file missing | Fallback to default voice | Voice pool with fallback chain |
| Intro/outro music missing | Plain generation (no music) | Check asset existence at startup; generate without if missing |
| MP3 encoding fails | Unplayable audio | Fallback to WAV (larger but functional); transcode later |

## Future Extensions

- ElevenLabs API integration for premium TTS (if user has API key)
- Neural voice cloning (custom voices)
- Music generation for intro/outro
- Dynamic ad insertion (sponsorship messages)
- Chapter markers in audio files
- Transcript generation (SRT/VTT for accessibility)
- Multi-language broadcast audio
- Podcast RSS feed generation (automatic podcast publishing)
- Audio speed options (1x, 1.5x, 2x)
- Smart audio compression (remove silences, speed up disfluencies)
