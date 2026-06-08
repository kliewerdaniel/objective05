use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A parsed audio script containing speaker-labeled segments.
#[derive(Debug, Clone)]
pub struct AudioScript {
    pub segments: Vec<AudioSegment>,
    pub metadata: AudioMetadata,
}

#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub speaker_id: String,
    pub text: String,
    pub segment_type: SegmentType,
    pub estimated_duration_seconds: f32,
}

#[derive(Debug, Clone, PartialEq)]
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

impl SegmentType {
    pub fn to_voice_style(&self) -> VoiceStyle {
        match self {
            Self::Intro => VoiceStyle::Anchor,
            Self::TopStory => VoiceStyle::Anchor,
            Self::NarrativeUpdate => VoiceStyle::Narrator,
            Self::ContradictionAlert => VoiceStyle::Analyst,
            Self::DeepDive => VoiceStyle::Correspondent,
            Self::Transition => VoiceStyle::Anchor,
            Self::Outro => VoiceStyle::Anchor,
            Self::IdleContent => VoiceStyle::Anchor,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioMetadata {
    pub title: String,
    pub broadcast_id: String,
    pub generated_at: DateTime<Utc>,
    pub word_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub gender: Option<String>,
    pub language: String,
    pub style: VoiceStyle,
    pub model_path: String,
    pub config_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VoiceStyle {
    Anchor,
    Correspondent,
    Narrator,
    Analyst,
}

#[derive(Debug, Clone)]
pub struct VoiceManager {
    pub voices: Vec<Voice>,
    pub default_voice_id: String,
}

#[derive(Debug, Clone)]
pub struct PodcastConfig {
    pub intro_path: Option<PathBuf>,
    pub outro_path: Option<PathBuf>,
    pub transition_paths: Vec<PathBuf>,
    pub crossfade_duration_ms: u32,
    pub silence_between_segments_ms: u32,
}

impl Default for PodcastConfig {
    fn default() -> Self {
        Self {
            intro_path: None,
            outro_path: None,
            transition_paths: Vec::new(),
            crossfade_duration_ms: 500,
            silence_between_segments_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRecord {
    pub id: String,
    pub broadcast_id: String,
    pub title: String,
    pub file_path: PathBuf,
    pub format: String,
    pub duration_seconds: u32,
    pub file_size_bytes: u64,
    pub sample_rate: u32,
    pub channels: u8,
    pub bitrate: String,
    pub word_count: usize,
    pub created_at: DateTime<Utc>,
}
