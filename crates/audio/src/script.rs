use crate::types::{AudioMetadata, AudioScript, AudioSegment, SegmentType};

pub struct ScriptParser {
    pub default_speaker: String,
}

impl Default for ScriptParser {
    fn default() -> Self {
        Self {
            default_speaker: "anchor".to_string(),
        }
    }
}

impl ScriptParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse broadcast markdown text into an AudioScript with speaker
    /// segments. Lines prefixed with `[SPEAKER]:` are assigned to that
    /// speaker; unlabeled lines continue with the last known speaker.
    pub fn parse(&self, text: &str, metadata: AudioMetadata) -> AudioScript {
        let mut segments = Vec::new();
        let mut current_speaker = self.default_speaker.clone();
        let mut current_text = String::new();
        let mut current_segment_type = SegmentType::TopStory;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                self.flush_segment(&mut segments, &current_speaker, &mut current_text, &current_segment_type);
                continue;
            }

            if let Some((speaker, rest)) = self.parse_speaker_line(trimmed) {
                self.flush_segment(&mut segments, &current_speaker, &mut current_text, &current_segment_type);
                current_speaker = speaker;
                current_segment_type = self.infer_segment_type(&current_speaker, rest);
                if !rest.is_empty() {
                    if !current_text.is_empty() {
                        current_text.push(' ');
                    }
                    current_text.push_str(rest);
                }
            } else {
                if !current_text.is_empty() {
                    current_text.push(' ');
                }
                current_text.push_str(trimmed);
            }
        }

        self.flush_segment(&mut segments, &current_speaker, &mut current_text, &current_segment_type);

        let word_count = segments.iter().map(|s| s.text.split_whitespace().count()).sum();
        AudioScript {
            metadata: AudioMetadata {
                word_count,
                ..metadata
            },
            segments,
        }
    }

    fn parse_speaker_line<'a>(&self, line: &'a str) -> Option<(String, &'a str)> {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            if let Some(bracket_end) = rest.find(']') {
                let speaker = rest[..bracket_end].to_lowercase();
                let after = rest[bracket_end + 1..].trim();
                let after = after.strip_prefix(':').unwrap_or(after).trim();
                return Some((speaker, after));
            }
        }
        None
    }

    fn infer_segment_type(&self, speaker: &str, text: &str) -> SegmentType {
        let lower = text.to_lowercase();
        if lower.contains("[intro]") || lower.contains("welcome to") {
            return SegmentType::Intro;
        }
        if lower.contains("[outro]") || lower.contains("concludes") || lower.contains("that's all") {
            return SegmentType::Outro;
        }
        match speaker {
            s if s == "anchor" => SegmentType::TopStory,
            s if s == "correspondent" => SegmentType::DeepDive,
            s if s == "narrator" => SegmentType::NarrativeUpdate,
            s if s == "analyst" => SegmentType::ContradictionAlert,
            _ => SegmentType::TopStory,
        }
    }

    fn flush_segment(
        &self,
        segments: &mut Vec<AudioSegment>,
        speaker: &str,
        text: &mut String,
        segment_type: &SegmentType,
    ) {
        let t = std::mem::take(text);
        if t.is_empty() {
            return;
        }
        let word_count = t.split_whitespace().count() as f32;
        let estimated_duration = word_count / 2.5;
        segments.push(AudioSegment {
            speaker_id: speaker.to_string(),
            text: t,
            segment_type: segment_type.clone(),
            estimated_duration_seconds: estimated_duration,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn parse_speaker_lines() {
        let parser = ScriptParser::new();
        let text = "[ANCHOR]: Welcome to the briefing.\n\n[REPORTER]: Latest from the field.\n\n[ANCHOR]: That's all.";
        let metadata = AudioMetadata {
            title: "Test".into(),
            broadcast_id: "b1".into(),
            generated_at: Utc::now(),
            word_count: 0,
        };
        let script = parser.parse(text, metadata);
        assert_eq!(script.segments.len(), 3);
        assert_eq!(script.segments[0].speaker_id, "anchor");
        assert_eq!(script.segments[1].speaker_id, "reporter");
        assert_eq!(script.segments[2].speaker_id, "anchor");
    }

    #[test]
    fn unlabeled_lines_use_default_speaker() {
        let parser = ScriptParser::new();
        let text = "[ANCHOR]: Hello.\nContinuing the thought.\n\nNext segment.";
        let metadata = AudioMetadata {
            title: "Test".into(),
            broadcast_id: "b1".into(),
            generated_at: Utc::now(),
            word_count: 0,
        };
        let script = parser.parse(text, metadata);
        assert_eq!(script.segments.len(), 2);
        assert_eq!(script.segments[0].speaker_id, "anchor");
        assert_eq!(script.segments[1].speaker_id, "anchor");
    }

    #[test]
    fn infer_segment_types() {
        let parser = ScriptParser::new();
        let text = "[ANCHOR]: Welcome to the briefing.\n\n[ANCHOR]: Top story today.\n\n[ANCHOR]: That concludes our briefing.";
        let metadata = AudioMetadata {
            title: "Test".into(),
            broadcast_id: "b1".into(),
            generated_at: Utc::now(),
            word_count: 0,
        };
        let script = parser.parse(text, metadata);
        assert_eq!(script.segments[0].segment_type, SegmentType::Intro);
        assert_eq!(script.segments[1].segment_type, SegmentType::TopStory);
        assert_eq!(script.segments[2].segment_type, SegmentType::Outro);
    }
}
