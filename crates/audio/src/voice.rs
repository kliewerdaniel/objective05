use std::sync::Arc;

use crate::types::{Voice, VoiceManager, VoiceStyle};

impl VoiceManager {
    pub fn new(voices: Vec<Voice>, default_voice_id: String) -> Self {
        Self {
            voices,
            default_voice_id,
        }
    }

    /// Built-in default voice pool for quick-start without configuration.
    pub fn builtin() -> Self {
        let voices = vec![
            Voice {
                id: "en_US-lessac-medium".into(),
                name: "James".into(),
                gender: Some("male".into()),
                language: "en-US".into(),
                style: VoiceStyle::Anchor,
                model_path: String::new(),
                config_path: String::new(),
            },
            Voice {
                id: "en_US-amy-medium".into(),
                name: "Amy".into(),
                gender: Some("female".into()),
                language: "en-US".into(),
                style: VoiceStyle::Correspondent,
                model_path: String::new(),
                config_path: String::new(),
            },
            Voice {
                id: "en_US-norman-medium".into(),
                name: "Norman".into(),
                gender: Some("male".into()),
                language: "en-US".into(),
                style: VoiceStyle::Narrator,
                model_path: String::new(),
                config_path: String::new(),
            },
        ];
        Self {
            voices,
            default_voice_id: "en_US-lessac-medium".into(),
        }
    }

    pub fn default(&self) -> Option<&Voice> {
        self.voices.iter().find(|v| v.id == self.default_voice_id)
    }
}

pub struct VoiceAssigner {
    manager: Arc<VoiceManager>,
}

impl VoiceAssigner {
    pub fn new(manager: Arc<VoiceManager>) -> Self {
        Self { manager }
    }

    /// Assign a voice for a given style, falling back through the chain.
    pub fn assign(&self, style: &VoiceStyle) -> Option<Voice> {
        let candidates: Vec<&Voice> = self
            .manager
            .voices
            .iter()
            .filter(|v| v.style == *style)
            .collect();

        if !candidates.is_empty() {
            return Some(candidates[0].clone());
        }

        self.manager.default().cloned()
    }
}
