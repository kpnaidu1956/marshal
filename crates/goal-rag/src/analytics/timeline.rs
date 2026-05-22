//! Workflow timeline reconstruction
//!
//! Reconstructs the complete sequence of events for a task or goal,
//! identifying phases, participants, and bottlenecks.

use chrono::{DateTime, Duration, Utc};
use std::collections::HashSet;
use uuid::Uuid;

use super::types::*;

/// Parameters for timeline reconstruction
pub struct ReconstructParams<'a> {
    pub organization_id: &'a str,
    pub entity_type: &'a str,
    pub entity_id: &'a str,
    pub classifications: &'a [InteractionClassification],
    pub entity_status: &'a str,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Timeline reconstructor for building workflow timelines
pub struct TimelineReconstructor;

impl TimelineReconstructor {
    pub fn new() -> Self {
        Self
    }

    /// Reconstruct timeline from classified interactions and activity events
    pub fn reconstruct(&self, params: ReconstructParams<'_>) -> WorkflowTimeline {
        let ReconstructParams {
            organization_id,
            entity_type,
            entity_id,
            classifications,
            entity_status,
            opened_at,
            closed_at,
        } = params;
        // Convert classifications to timeline events (activity events are now classified too)
        let mut all_events = self.merge_events(classifications);
        all_events.sort_by_key(|e| e.timestamp);

        // Identify unique participants
        let participants: HashSet<String> = all_events
            .iter()
            .map(|e| e.actor_id.clone())
            .collect();

        // Identify phases
        let phases = self.identify_phases(&all_events, opened_at, closed_at);

        // Detect bottlenecks
        let bottlenecks = self.detect_bottlenecks(&all_events, classifications);

        // Calculate duration
        let total_duration_hours = closed_at.map(|end| {
            let duration = end - opened_at;
            duration.num_hours() as f64 + (duration.num_minutes() % 60) as f64 / 60.0
        });

        WorkflowTimeline {
            id: Uuid::new_v4(),
            organization_id: organization_id.to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            total_interactions: classifications.len() as u32,
            total_participants: participants.len() as u32,
            total_duration_hours,
            phases,
            key_events: all_events,
            bottlenecks,
            status: entity_status.to_string(),
            opened_at,
            closed_at,
            last_analyzed_at: Utc::now(),
        }
    }

    /// Convert classifications into timeline events
    fn merge_events(
        &self,
        classifications: &[InteractionClassification],
    ) -> Vec<TimelineEvent> {
        classifications
            .iter()
            .map(|c| {
                let description = format!(
                    "{}: {}",
                    c.interaction_type.as_str(),
                    truncate_content(&c.content, 100)
                );

                TimelineEvent {
                    timestamp: c.original_created_at,
                    event_type: format!("interaction:{}", c.interaction_type.as_str()),
                    description,
                    actor_id: c.sender_id.clone(),
                    actor_name: None,
                    interaction_id: Some(c.id.to_string()),
                    metadata: Some(serde_json::json!({
                        "source_type": c.source_type.as_str(),
                        "confidence": c.confidence_score,
                        "urgency": c.urgency_level.as_str(),
                    })),
                }
            })
            .collect()
    }

    /// Identify workflow phases based on events
    fn identify_phases(
        &self,
        events: &[TimelineEvent],
        opened_at: DateTime<Utc>,
        closed_at: Option<DateTime<Utc>>,
    ) -> Vec<WorkflowPhase> {
        if events.is_empty() {
            return vec![WorkflowPhase {
                name: "created".to_string(),
                start: opened_at,
                end: closed_at,
                interaction_count: 0,
                participants: vec![],
            }];
        }

        let mut phases = Vec::new();
        let mut current_phase_name = "initiated".to_string();
        let mut current_phase_start = opened_at;
        let mut current_interactions = 0u32;
        let mut current_participants: HashSet<String> = HashSet::new();

        for event in events {
            // Detect phase transitions based on event types
            let new_phase = self.detect_phase_transition(&event.event_type, &current_phase_name);

            if let Some(phase_name) = new_phase {
                // End current phase
                phases.push(WorkflowPhase {
                    name: current_phase_name,
                    start: current_phase_start,
                    end: Some(event.timestamp),
                    interaction_count: current_interactions,
                    participants: current_participants.into_iter().collect(),
                });

                // Start new phase
                current_phase_name = phase_name;
                current_phase_start = event.timestamp;
                current_interactions = 0;
                current_participants = HashSet::new();
            }

            current_interactions += 1;
            current_participants.insert(event.actor_id.clone());
        }

        // End final phase
        phases.push(WorkflowPhase {
            name: current_phase_name,
            start: current_phase_start,
            end: closed_at,
            interaction_count: current_interactions,
            participants: current_participants.into_iter().collect(),
        });

        phases
    }

    /// Detect if an event triggers a phase transition
    fn detect_phase_transition(&self, event_type: &str, current_phase: &str) -> Option<String> {
        match event_type {
            // Assignment (activity events classified as assignment)
            "interaction:assignment" => Some("assigned".to_string()),

            // Progress (status updates from activity events or comments)
            // Includes "escalated" so the timeline can recover from escalation
            "interaction:status_update" if current_phase == "assigned"
                || current_phase == "initiated"
                || current_phase == "blocked"
                || current_phase == "escalated" =>
            {
                Some("in_progress".to_string())
            }

            // Approval flow
            "interaction:request_approval" if current_phase != "review" && current_phase != "pending_approval" => {
                Some("pending_approval".to_string())
            }

            // Blockers
            "interaction:blocker" if current_phase != "blocked" => Some("blocked".to_string()),

            // Escalation
            "interaction:escalation" if current_phase != "escalated" => Some("escalated".to_string()),

            // Recognition / acknowledgment after approval request can signal approval
            "interaction:acknowledgment" if current_phase == "pending_approval" => {
                Some("approved".to_string())
            }

            _ => None,
        }
    }

    /// Detect bottlenecks in the workflow
    fn detect_bottlenecks(
        &self,
        events: &[TimelineEvent],
        classifications: &[InteractionClassification],
    ) -> Vec<WorkflowBottleneck> {
        let mut bottlenecks = Vec::new();

        // 1. Detect approval delays (request_approval -> approved > 24h)
        if let Some(bottleneck) = self.detect_approval_delay(events) {
            bottlenecks.push(bottleneck);
        }

        // 2. Detect blocked periods
        if let Some(bottleneck) = self.detect_blocked_period(events, classifications) {
            bottlenecks.push(bottleneck);
        }

        // 3. Detect communication gaps (no activity > 48h)
        bottlenecks.extend(self.detect_communication_gaps(events));

        // 4. Detect clarification loops (multiple clarification requests)
        if let Some(bottleneck) = self.detect_clarification_loop(classifications) {
            bottlenecks.push(bottleneck);
        }

        bottlenecks
    }

    fn detect_approval_delay(&self, events: &[TimelineEvent]) -> Option<WorkflowBottleneck> {
        let mut approval_request_time: Option<DateTime<Utc>> = None;

        for event in events {
            if event.event_type.contains("request_approval") || event.event_type.contains("review_requested") {
                approval_request_time = Some(event.timestamp);
            } else if event.event_type.contains("approved") {
                if let Some(request_time) = approval_request_time {
                    let delay = event.timestamp - request_time;
                    if delay > Duration::hours(24) {
                        return Some(WorkflowBottleneck {
                            bottleneck_type: "approval_delay".to_string(),
                            duration_hours: delay.num_hours() as f64,
                            description: format!(
                                "Approval took {:.1} hours (threshold: 24h)",
                                delay.num_hours() as f64
                            ),
                            start: request_time,
                            end: event.timestamp,
                            caused_by: None,
                        });
                    }
                }
                approval_request_time = None;
            }
        }

        None
    }

    fn detect_blocked_period(
        &self,
        events: &[TimelineEvent],
        classifications: &[InteractionClassification],
    ) -> Option<WorkflowBottleneck> {
        // Find blocker interactions
        let blocker_time = classifications.iter()
            .find(|c| c.interaction_type == InteractionType::Blocker)
            .map(|c| c.original_created_at);

        if let Some(blocked_start) = blocker_time {
            // Find when blocking was resolved
            let resolution_time = events.iter()
                .filter(|e| e.timestamp > blocked_start)
                .find(|e| {
                    e.event_type.contains("unblocked") ||
                    e.event_type.contains("resolved") ||
                    e.event_type.contains("in_progress")
                })
                .map(|e| e.timestamp);

            if let Some(end) = resolution_time {
                let duration = end - blocked_start;
                if duration > Duration::hours(4) {
                    return Some(WorkflowBottleneck {
                        bottleneck_type: "blocked_period".to_string(),
                        duration_hours: duration.num_hours() as f64,
                        description: format!("Task was blocked for {:.1} hours", duration.num_hours() as f64),
                        start: blocked_start,
                        end,
                        caused_by: classifications.iter()
                            .find(|c| c.interaction_type == InteractionType::Blocker)
                            .and_then(|c| c.entities.blockers.first().cloned()),
                    });
                }
            }
        }

        None
    }

    fn detect_communication_gaps(&self, events: &[TimelineEvent]) -> Vec<WorkflowBottleneck> {
        let mut gaps = Vec::new();

        if events.len() < 2 {
            return gaps;
        }

        for window in events.windows(2) {
            let gap = window[1].timestamp - window[0].timestamp;
            if gap > Duration::hours(48) {
                gaps.push(WorkflowBottleneck {
                    bottleneck_type: "communication_gap".to_string(),
                    duration_hours: gap.num_hours() as f64,
                    description: format!("No activity for {:.1} hours", gap.num_hours() as f64),
                    start: window[0].timestamp,
                    end: window[1].timestamp,
                    caused_by: None,
                });
            }
        }

        gaps
    }

    fn detect_clarification_loop(
        &self,
        classifications: &[InteractionClassification],
    ) -> Option<WorkflowBottleneck> {
        let clarifications: Vec<_> = classifications.iter()
            .filter(|c| c.interaction_type == InteractionType::RequestClarification)
            .collect();

        let clarification_count = clarifications.len();

        if clarification_count >= 3 {
            // Safe to unwrap since we know count >= 3
            let first = clarifications.first().unwrap();
            let last = clarifications.last().unwrap();

            // Calculate duration more precisely, handling same-timestamp edge case
            let duration = last.original_created_at - first.original_created_at;
            let duration_hours = duration.num_hours() as f64
                + (duration.num_minutes() % 60) as f64 / 60.0;

            return Some(WorkflowBottleneck {
                bottleneck_type: "clarification_loop".to_string(),
                duration_hours,
                description: format!(
                    "{} clarification requests detected - consider improving initial specifications",
                    clarification_count
                ),
                start: first.original_created_at,
                end: last.original_created_at,
                caused_by: Some("Unclear requirements".to_string()),
            });
        }

        None
    }
}

impl Default for TimelineReconstructor {
    fn default() -> Self {
        Self::new()
    }
}

/// Activity event from task/goal activity logs
#[derive(Debug, Clone)]
pub struct ActivityEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub description: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub changes: Option<serde_json::Value>,
}

/// Truncate content for display
fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else if max_len <= 3 {
        // If max_len is too small for "...", just return truncated content
        content.chars().take(max_len).collect()
    } else {
        // Find a valid UTF-8 character boundary
        let truncate_at = max_len - 3;
        let mut end = truncate_at;

        // Walk backwards to find a valid char boundary
        while end > 0 && !content.is_char_boundary(end) {
            end -= 1;
        }

        format!("{}...", &content[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_detection() {
        let reconstructor = TimelineReconstructor::new();

        assert_eq!(
            reconstructor.detect_phase_transition("interaction:assignment", "initiated"),
            Some("assigned".to_string())
        );

        assert_eq!(
            reconstructor.detect_phase_transition("interaction:status_update", "assigned"),
            Some("in_progress".to_string())
        );

        assert_eq!(
            reconstructor.detect_phase_transition("interaction:blocker", "in_progress"),
            Some("blocked".to_string())
        );

        assert_eq!(
            reconstructor.detect_phase_transition("interaction:request_approval", "in_progress"),
            Some("pending_approval".to_string())
        );

        assert_eq!(
            reconstructor.detect_phase_transition("interaction:escalation", "in_progress"),
            Some("escalated".to_string())
        );

        // Escalated phase can recover via status_update
        assert_eq!(
            reconstructor.detect_phase_transition("interaction:status_update", "escalated"),
            Some("in_progress".to_string())
        );

        // Escalation should not re-trigger when already escalated
        assert_eq!(
            reconstructor.detect_phase_transition("interaction:escalation", "escalated"),
            None
        );

        // No transition for unrecognized types
        assert_eq!(
            reconstructor.detect_phase_transition("interaction:other", "in_progress"),
            None
        );
    }
}
