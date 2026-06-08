use async_trait::async_trait;
use objective_core::{ObjectiveError, Result};
use std::process::Stdio;
use tokio::process::Command;

use crate::engine::AsrEngine;
use crate::types::{AsrConfig, Transcription, TranscriptionSegment};

/// Whisper.cpp-based ASR engine.
///
/// Calls the `whisper.cpp` CLI (`whisper-cli` or `main` binary) as a
/// subprocess. The binary must be installed separately and available on
/// `PATH`, or configured via `WHISPER_CLI_PATH` environment variable.
pub struct WhisperAsrEngine {
    config: AsrConfig,
    cli_path: String,
    model_path: String,
}

impl WhisperAsrEngine {
    /// Create a new Whisper ASR engine.
    ///
    /// `cli_path` — path to the whisper.cpp CLI binary (default: `whisper-cli`).
    /// `model_path` — path to the GGML model file.
    pub fn new(config: AsrConfig, cli_path: String, model_path: String) -> Self {
        Self {
            config,
            cli_path,
            model_path,
        }
    }

    /// Auto-discover the whisper CLI path from environment or defaults.
    pub fn with_defaults(config: AsrConfig) -> Self {
        let cli_path = std::env::var("WHISPER_CLI_PATH")
            .unwrap_or_else(|_| "whisper-cli".to_string());
        let model_path = std::env::var("WHISPER_MODEL_PATH")
            .unwrap_or_else(|_| format!("~/.objective/models/ggml-{}.bin", config.model));
        Self::new(config, cli_path, model_path)
    }
}

#[async_trait]
impl AsrEngine for WhisperAsrEngine {
    async fn transcribe(&self, audio_bytes: &[u8]) -> Result<Transcription> {
        // Write audio bytes to a temp file
        let temp_dir = std::env::temp_dir().join(format!("objective-asr-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| ObjectiveError::Storage(format!("create temp dir: {e}")))?;
        let input_path = temp_dir.join("input.wav");
        let output_path = temp_dir.join("output.json");
        std::fs::write(&input_path, audio_bytes)
            .map_err(|e| ObjectiveError::Storage(format!("write temp audio: {e}")))?;

        let start = std::time::Instant::now();

        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-m")
            .arg(&self.model_path)
            .arg("-f")
            .arg(&input_path)
            .arg("-oj")
            .arg("-of")
            .arg(output_path.to_string_lossy().as_ref())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !self.config.language.is_empty() {
            cmd.arg("-l").arg(&self.config.language);
        }
        if self.config.translate_to_english {
            cmd.arg("-tr");
        }
        cmd.arg("-t").arg(self.config.threads.to_string());

        let output = cmd.output().await.map_err(|e| {
            ObjectiveError::Runtime(format!(
                "whisper CLI failed to start (is '{}' installed?): {e}",
                self.cli_path
            ))
        })?;

        let processing_ms = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ObjectiveError::Runtime(format!(
                "whisper CLI exited with status {}: {}",
                output.status, stderr
            )));
        }

        let json_path = temp_dir.join("output.json");
        let json_str = std::fs::read_to_string(&json_path)
            .map_err(|e| ObjectiveError::Storage(format!("read whisper output: {e}")))?;

        // Clean up temp files
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Parse whisper.cpp JSON output
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| ObjectiveError::Storage(format!("parse whisper output: {e}")))?;

        let text = parsed["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        let language = parsed["language"]
            .as_str()
            .unwrap_or("en")
            .to_string();
        let duration = parsed["duration"]
            .as_f64()
            .unwrap_or(0.0) as f32;

        let segments: Vec<TranscriptionSegment> = parsed["segments"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| {
                        Some(TranscriptionSegment {
                            start: s["start"].as_f64()? as f32,
                            end: s["end"].as_f64()? as f32,
                            text: s["text"].as_str()?.to_string(),
                            confidence: s.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.9) as f32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Transcription {
            text,
            language,
            duration_seconds: duration,
            segments,
            processing_time_ms: processing_ms,
        })
    }

    fn config(&self) -> &AsrConfig {
        &self.config
    }
}
