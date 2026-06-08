use serde::{Deserialize, Serialize};

/// Full transcription result from an ASR engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    /// Full transcribed text.
    pub text: String,
    /// Detected language (BCP-47 code, e.g. "en").
    pub language: String,
    /// Duration of the audio in seconds.
    pub duration_seconds: f32,
    /// Per-segment transcription with timing.
    pub segments: Vec<TranscriptionSegment>,
    /// Processing time for the ASR model.
    pub processing_time_ms: u64,
}

/// A single segment of transcribed speech with timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    /// Start time in seconds.
    pub start: f32,
    /// End time in seconds.
    pub end: f32,
    /// Transcribed text for this segment.
    pub text: String,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
}

/// Configuration for the ASR engine.
#[derive(Debug, Clone)]
pub struct AsrConfig {
    /// Model identifier (e.g. "base.en", "small.en").
    pub model: String,
    /// Language override (BCP-47, empty = auto-detect).
    pub language: String,
    /// Number of CPU threads for inference.
    pub threads: u32,
    /// Whether to translate to English (whisper translate mode).
    pub translate_to_english: bool,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            model: "base.en".to_string(),
            language: String::new(),
            threads: 4,
            translate_to_english: false,
        }
    }
}
