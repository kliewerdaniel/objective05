use async_trait::async_trait;
use objective_core::Result;

use crate::types::{AsrConfig, Transcription};

#[async_trait]
pub trait AsrEngine: Send + Sync {
    /// Transcribe audio bytes and return the transcription.
    ///
    /// `audio_bytes` should be a supported format (WAV, MP3, OGG, etc.).
    async fn transcribe(&self, audio_bytes: &[u8]) -> Result<Transcription>;

    /// Return the active configuration.
    fn config(&self) -> &AsrConfig;
}

/// Stub ASR engine — logs what it would transcribe, returns a placeholder.
/// Used when no real ASR backend is configured.
pub struct StubAsrEngine {
    config: AsrConfig,
}

impl StubAsrEngine {
    pub fn new(config: AsrConfig) -> Self {
        Self { config }
    }
}

impl Default for StubAsrEngine {
    fn default() -> Self {
        Self {
            config: AsrConfig::default(),
        }
    }
}

#[async_trait]
impl AsrEngine for StubAsrEngine {
    async fn transcribe(&self, audio_bytes: &[u8]) -> Result<Transcription> {
        let estimated_seconds = (audio_bytes.len() as f32) / 16000.0 / 2.0;
        tracing::info!(
            audio_bytes = audio_bytes.len(),
            estimated_duration_s = estimated_seconds,
            model = %self.config.model,
            "stub-asr: would transcribe audio"
        );
        Ok(Transcription {
            text: format!(
                "[Stub ASR transcription would appear here. \
                 Audio: {} bytes, ~{:.1}s. Enable the `whisper` feature \
                 and configure whisper.cpp to generate real transcriptions.]",
                audio_bytes.len(),
                estimated_seconds,
            ),
            language: self.config.language.clone(),
            duration_seconds: estimated_seconds,
            segments: Vec::new(),
            processing_time_ms: 0,
        })
    }

    fn config(&self) -> &AsrConfig {
        &self.config
    }
}
