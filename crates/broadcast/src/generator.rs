use std::sync::Arc;

use objective_core::traits::{InferenceKind, InferenceTask, ModelId, ModelRuntime};

use crate::types::{BroadcastCollection, BroadcastRecord};

pub struct BroadcastGenerator {
    runtime: Option<Arc<dyn ModelRuntime>>,
}

impl BroadcastGenerator {
    pub fn new(runtime: Option<Arc<dyn ModelRuntime>>) -> Self {
        Self { runtime }
    }

    pub async fn generate(
        &self,
        collection: &BroadcastCollection,
    ) -> objective_core::Result<BroadcastRecord> {
        if let Some(runtime) = &self.runtime {
            self.generate_with_llm(runtime, collection).await
        } else {
            Ok(self.generate_template(collection))
        }
    }

    async fn generate_with_llm(
        &self,
        runtime: &Arc<dyn ModelRuntime>,
        collection: &BroadcastCollection,
    ) -> objective_core::Result<BroadcastRecord> {
        let prompt = self.build_prompt(collection);

        let task = InferenceTask::new(
            ModelId::Custom("extraction_llm".to_string()),
            InferenceKind::ReportDrafting,
            prompt,
        )
        .with_timeout(std::time::Duration::from_secs(120));

        match runtime.infer(task).await {
            Ok(result) => {
                let body = if result.text.is_empty() {
                    self.generate_template(collection).body_markdown
                } else {
                    result.text
                };
                let event_count = collection.top_events.len()
                    + collection.narratives.len()
                    + collection.contradictions.len();
                let title =
                    if collection.top_events.is_empty() {
                        "Objective Pulse — Idle Report".to_string()
                    } else {
                        format!(
                            "Objective Pulse — {}",
                            chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
                        )
                    };
                let summary = collection
                    .top_events
                    .first()
                    .map(|e| format!("Top story: {}", e.title))
                    .unwrap_or_else(|| "No significant events to report.".to_string());

                Ok(BroadcastRecord::new(title, summary, body, event_count))
            }
            Err(_) => Ok(self.generate_template(collection)),
        }
    }

    fn generate_template(&self, collection: &BroadcastCollection) -> BroadcastRecord {
        let now = chrono::Utc::now();
        let date_str = now.format("%Y-%m-%d %H:%M UTC").to_string();

        let event_count = collection.top_events.len()
            + collection.narratives.len()
            + collection.contradictions.len();

        let title = if collection.top_events.is_empty() {
            "Objective Pulse — Idle Report".to_string()
        } else {
            format!("Objective Pulse — {}", date_str)
        };

        let mut body = String::new();
        body.push_str(&format!("# {}\n\n", title));
        body.push_str(&format!("_Generated on {}_\n\n", date_str));

        if collection.top_events.is_empty() {
            body.push_str("## System Status\n\n");
            body.push_str("No significant events detected since the last report. ");
            body.push_str("The system is monitoring all configured sources.\n\n");
            body.push_str("### Tracked Items\n\n");
            if !collection.narratives.is_empty() {
                body.push_str(&format!(
                    "- **Narratives:** {} active narratives being tracked\n",
                    collection.narratives.len()
                ));
            }
            body.push_str("- **Events:** No new events to report\n");
            return BroadcastRecord::new(
                title,
                "No significant events to report.".to_string(),
                body,
                event_count,
            );
        }

        body.push_str("## Top Stories\n\n");
        for (i, event) in collection.top_events.iter().enumerate() {
            body.push_str(&format!("### {}. {}\n\n", i + 1, event.title));
            body.push_str(&format!("{}\n\n", event.description));
            body.push_str(&format!("- **Importance:** {:.2}\n", event.importance));
            body.push_str(&format!("- **Confidence:** {:.2}\n", event.confidence));
            body.push_str(&format!("- **Claims:** {}\n", event.claim_count));
            if !event.entities.is_empty() {
                body.push_str(&format!(
                    "- **Entities:** {}\n",
                    event.entities.join(", ")
                ));
            }
            body.push('\n');
        }

        if !collection.contradictions.is_empty() {
            body.push_str("## Contradictions\n\n");
            for c in &collection.contradictions {
                body.push_str(&format!(
                    "- **{}:** {} (severity: {:.2})\n",
                    c.entity_name, c.description, c.severity
                ));
            }
            body.push('\n');
        }

        body.push_str("---\n\n");
        body.push_str(&format!(
            "_This broadcast covers {} events, {} narratives, and {} contradictions._\n",
            collection.top_events.len(),
            collection.narratives.len(),
            collection.contradictions.len()
        ));

        let summary = collection
            .top_events
            .first()
            .map(|e| format!("Top story: {}", e.title))
            .unwrap_or_else(|| "No significant events to report.".to_string());

        BroadcastRecord::new(title, summary, body, event_count)
    }

    fn build_prompt(&self, collection: &BroadcastCollection) -> String {
        let mut prompt =
            String::from("Generate a concise intelligence broadcast in Markdown format based on the following events.\n\n");
        prompt.push_str("## Events\n\n");
        for event in &collection.top_events {
            prompt.push_str(&format!(
                "- **{}** (importance: {:.2}, confidence: {:.2})\n",
                event.title, event.importance, event.confidence
            ));
            prompt.push_str(&format!("  Description: {}\n", event.description));
            if !event.entities.is_empty() {
                prompt.push_str(&format!(
                    "  Entities: {}\n",
                    event.entities.join(", ")
                ));
            }
            prompt.push('\n');
        }
        prompt.push_str("\nFormat the broadcast with:\n");
        prompt.push_str("- A headline section\n");
        prompt.push_str("- Individual stories with key facts\n");
        prompt.push_str("- Source attribution where applicable\n");
        prompt.push_str("- Maintain a neutral, factual tone\n");
        prompt
    }
}
