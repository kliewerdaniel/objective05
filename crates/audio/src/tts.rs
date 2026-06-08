use async_trait::async_trait;
use objective_core::Result;

use crate::types::AudioSegment;

/// Audio data produced by a TTS engine.
pub struct TtsOutput {
    /// WAV or raw PCM bytes.
    pub audio_bytes: Vec<u8>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Duration in seconds (estimated or exact).
    pub duration_seconds: f32,
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    /// Synthesize speech for a single audio segment.
    async fn synthesize(&self, segment: &AudioSegment) -> Result<TtsOutput>;
}

/// Stub TTS engine — logs what it would generate, returns silence.
/// Used when no real TTS backend is configured.
pub struct StubTtsEngine;

#[async_trait]
impl TtsEngine for StubTtsEngine {
    async fn synthesize(&self, segment: &AudioSegment) -> Result<TtsOutput> {
        let word_count = segment.text.split_whitespace().count().max(1);
        let duration_seconds = word_count as f32 / 2.5;
        tracing::info!(
            speaker = %segment.speaker_id,
            words = word_count,
            duration_s = duration_seconds,
            text = %segment.text.chars().take(80).collect::<String>(),
            "stub-tts: would synthesize speech"
        );
        let sample_rate = 22050;
        let num_samples = (sample_rate as f32 * duration_seconds) as usize;
        let audio_bytes = vec![0u8; num_samples * 2];
        Ok(TtsOutput {
            audio_bytes,
            sample_rate,
            duration_seconds,
        })
    }
}
