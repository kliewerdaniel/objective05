use objective_core::Result;

use crate::types::{AudioScript, PodcastConfig};

/// Audio assembler combines synthesized segments into a final audio file.
/// The current implementation is a stub that logs the assembly steps.
pub struct AudioAssembler {
    pub config: PodcastConfig,
}

impl AudioAssembler {
    pub fn new(config: PodcastConfig) -> Self {
        Self { config }
    }

    pub async fn assemble(&self, script: &AudioScript, output_path: &std::path::Path) -> Result<()> {
        tracing::info!(
            segments = script.segments.len(),
            path = %output_path.display(),
            "assembling audio (stub)"
        );
        Ok(())
    }
}
