use std::sync::Arc;

use objective_core::traits::MessageBus;
use objective_core::types::EventEnvelope;
use objective_core::Result;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::store::AudioRepository;
use crate::tts::{StubTtsEngine, TtsEngine};
use crate::types::{AudioMetadata, AudioRecord, PodcastConfig, VoiceManager};
use crate::voice::VoiceAssigner;

pub struct AudioService {
    tts_engine: Arc<dyn TtsEngine>,
    repository: Arc<dyn AudioRepository>,
    bus: Arc<dyn MessageBus>,
    voice_assigner: Arc<VoiceAssigner>,
    podcast_config: PodcastConfig,
    audio_dir: std::path::PathBuf,
}

impl AudioService {
    pub fn new(
        tts_engine: Arc<dyn TtsEngine>,
        repository: Arc<dyn AudioRepository>,
        bus: Arc<dyn MessageBus>,
        voice_assigner: Arc<VoiceAssigner>,
        podcast_config: PodcastConfig,
        audio_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            tts_engine,
            repository,
            bus,
            voice_assigner,
            podcast_config,
            audio_dir,
        }
    }

    pub fn new_stub(
        repository: Arc<dyn AudioRepository>,
        bus: Arc<dyn MessageBus>,
        audio_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            tts_engine: Arc::new(StubTtsEngine),
            repository,
            bus,
            voice_assigner: Arc::new(crate::voice::VoiceAssigner::new(Arc::new(
                VoiceManager::builtin(),
            ))),
            podcast_config: PodcastConfig::default(),
            audio_dir,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("audio service starting (stub mode)");
        let mut last_index: usize = 0;
        let mut ticker = interval(Duration::from_secs(10));

        loop {
            ticker.tick().await;

            let events = self.bus.events().await?;
            if events.len() <= last_index {
                continue;
            }

            let new_events: Vec<EventEnvelope> = events[last_index..]
                .iter()
                .map(|(_, e)| e.clone())
                .collect();
            last_index = events.len();

            for event in new_events {
                if event.event_type == "broadcast.generated" {
                    if let Err(e) = self.process_broadcast(event).await {
                        error!(error = %e, "audio processing failed");
                    }
                }
            }
        }
    }

    async fn process_broadcast(&self, event: EventEnvelope) -> Result<()> {
        let broadcast_id = event
            .data
            .get("broadcast_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let title = event
            .data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let body_markdown = event
            .data
            .get("body_markdown")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let metadata = AudioMetadata {
            title: title.to_string(),
            broadcast_id: broadcast_id.to_string(),
            generated_at: chrono::Utc::now(),
            word_count: 0,
        };

        let script = crate::script::ScriptParser::new().parse(body_markdown, metadata);
        info!(
            broadcast_id = %broadcast_id,
            segments = script.segments.len(),
            "processing broadcast for audio"
        );

        let date_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let audio_broadcasts_dir = self.audio_dir.join("broadcasts").join(&date_str);
        std::fs::create_dir_all(&audio_broadcasts_dir)
            .map_err(|e| objective_core::ObjectiveError::Storage(format!("create audio dir: {e}")))?;

        for segment in &script.segments {
            let assigned = self.voice_assigner.assign(&segment.segment_type.to_voice_style());
            info!(
                speaker = %segment.speaker_id,
                voice = ?assigned.map(|v| v.id),
                type = ?segment.segment_type,
                words = segment.text.split_whitespace().count(),
                "synthesizing segment"
            );
            self.tts_engine.synthesize(segment).await?;
        }

        let assembler = crate::assembler::AudioAssembler::new(self.podcast_config.clone());
        let output_path = audio_broadcasts_dir.join(format!("{}.mp3", broadcast_id));
        assembler.assemble(&script, &output_path).await?;

        let record = AudioRecord {
            id: ulid::Ulid::new().to_string(),
            broadcast_id: broadcast_id.to_string(),
            title: title.to_string(),
            file_path: output_path,
            format: "mp3".to_string(),
            duration_seconds: 0,
            file_size_bytes: 0,
            sample_rate: 22050,
            channels: 1,
            bitrate: "128k".to_string(),
            word_count: script.metadata.word_count,
            created_at: chrono::Utc::now(),
        };
        self.repository.insert(record.clone()).await?;

        let _ = self
            .bus
            .publish(
                "audio.broadcast.ready",
                EventEnvelope::new(
                    "audio.broadcast.ready",
                    "audio",
                    serde_json::json!({
                        "audio_id": record.id,
                        "broadcast_id": record.broadcast_id,
                        "title": record.title,
                        "duration_seconds": record.duration_seconds,
                        "file_path": record.file_path.to_string_lossy().to_string(),
                    }),
                ),
            )
            .await;

        info!(
            audio_id = %record.id,
            broadcast_id = %record.broadcast_id,
            "audio broadcast ready"
        );

        Ok(())
    }
}
