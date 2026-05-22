//! Recommendation generator
//!
//! Generates efficiency recommendations based on learned patterns
//! and current workflow analysis.

use chrono::Utc;
use uuid::Uuid;

use super::types::*;

/// Recommendation generator
#[allow(dead_code)]
pub struct Recommender {
    /// LLM for generating detailed recommendations (optional, for future use)
    use_llm: bool,
}

impl Recommender {
    pub fn new(use_llm: bool) -> Self {
        Self { use_llm }
    }

    pub fn basic() -> Self {
        Self::new(false)
    }

    /// Generate recommendations for a specific timeline based on patterns
    pub fn generate_for_timeline(
        &self,
        timeline: &WorkflowTimeline,
        patterns: &[WorkflowPattern],
    ) -> Vec<EfficiencyRecommendation> {
        let mut recommendations = Vec::new();

        // Check for active bottleneck patterns that match
        for pattern in patterns.iter().filter(|p| p.is_active) {
            if let Some(rec) = self.match_pattern_to_timeline(timeline, pattern) {
                recommendations.push(rec);
            }
        }

        // Generate bottleneck-specific recommendations
        for bottleneck in &timeline.bottlenecks {
            recommendations.push(self.generate_bottleneck_recommendation(timeline, bottleneck));
        }

        // Check for process improvements
        if let Some(rec) = self.check_clarification_overload(timeline) {
            recommendations.push(rec);
        }

        if let Some(rec) = self.check_approval_process(timeline) {
            recommendations.push(rec);
        }

        // Deduplicate similar recommendations
        self.deduplicate_recommendations(recommendations)
    }

    /// Generate organization-wide recommendations
    pub fn generate_org_recommendations(
        &self,
        organization_id: &str,
        patterns: &[WorkflowPattern],
        recent_timelines: &[WorkflowTimeline],
    ) -> Vec<EfficiencyRecommendation> {
        let mut recommendations = Vec::new();

        // Recommendations based on common bottleneck patterns
        for pattern in patterns.iter().filter(|p| p.pattern_type == PatternType::Bottleneck && p.is_active) {
            if pattern.occurrence_count >= 5 {
                recommendations.push(EfficiencyRecommendation {
                    id: Uuid::new_v4(),
                    organization_id: organization_id.to_string(),
                    target_type: RecommendationTarget::Organization,
                    target_id: None,
                    recommendation_type: RecommendationType::Process,
                    title: format!("Address recurring {} bottleneck", pattern.pattern_name),
                    description: pattern.description.clone(),
                    suggested_actions: self.generate_bottleneck_actions(&pattern.pattern_name),
                    based_on_patterns: vec![pattern.id.to_string()],
                    evidence: serde_json::json!({
                        "occurrence_count": pattern.occurrence_count,
                        "avg_time_impact_hours": pattern.avg_time_impact_hours,
                        "affected_tasks": pattern.examples.len(),
                    }),
                    priority: if pattern.avg_time_impact_hours.unwrap_or(0.0) > 24.0 {
                        UrgencyLevel::High
                    } else {
                        UrgencyLevel::Medium
                    },
                    estimated_time_savings_hours: pattern.avg_time_impact_hours
                        .map(|h| h * 0.5 * pattern.occurrence_count as f64),
                    status: RecommendationStatus::Pending,
                    user_feedback: None,
                    generated_at: Utc::now(),
                });
            }
        }

        // Recommendations based on efficiency patterns
        for pattern in patterns.iter().filter(|p| p.pattern_type == PatternType::Efficiency && p.is_active) {
            if pattern.confidence_score > 0.7 {
                recommendations.push(EfficiencyRecommendation {
                    id: Uuid::new_v4(),
                    organization_id: organization_id.to_string(),
                    target_type: RecommendationTarget::Organization,
                    target_id: None,
                    recommendation_type: RecommendationType::Process,
                    title: format!("Apply {} pattern", pattern.pattern_name),
                    description: format!(
                        "Consider standardizing this successful pattern: {}",
                        pattern.description
                    ),
                    suggested_actions: vec![
                        "Review successful task examples".to_string(),
                        "Document the pattern as a best practice".to_string(),
                        "Share with team during planning".to_string(),
                    ],
                    based_on_patterns: vec![pattern.id.to_string()],
                    evidence: serde_json::json!({
                        "pattern_confidence": pattern.confidence_score,
                        "examples": pattern.examples.len(),
                    }),
                    priority: UrgencyLevel::Low,
                    estimated_time_savings_hours: pattern.avg_time_impact_hours,
                    status: RecommendationStatus::Pending,
                    user_feedback: None,
                    generated_at: Utc::now(),
                });
            }
        }

        // Check for systemic issues in recent timelines
        let communication_gaps = recent_timelines.iter()
            .flat_map(|t| t.bottlenecks.iter())
            .filter(|b| b.bottleneck_type == "communication_gap")
            .count();

        if communication_gaps >= 3 {
            recommendations.push(EfficiencyRecommendation {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                target_type: RecommendationTarget::Organization,
                target_id: None,
                recommendation_type: RecommendationType::Communication,
                title: "Improve team communication frequency".to_string(),
                description: format!(
                    "Detected {} communication gaps (>48h silence) in recent tasks. Consider implementing regular check-ins.",
                    communication_gaps
                ),
                suggested_actions: vec![
                    "Implement daily async standup updates".to_string(),
                    "Set up automated progress reminders".to_string(),
                    "Create a team communication channel for quick updates".to_string(),
                ],
                based_on_patterns: vec![],
                evidence: serde_json::json!({
                    "communication_gaps_detected": communication_gaps,
                    "recent_tasks_analyzed": recent_timelines.len(),
                }),
                priority: UrgencyLevel::Medium,
                estimated_time_savings_hours: Some(communication_gaps as f64 * 8.0),
                status: RecommendationStatus::Pending,
                user_feedback: None,
                generated_at: Utc::now(),
            });
        }

        recommendations
    }

    /// Match a pattern to a timeline and generate recommendation if applicable
    fn match_pattern_to_timeline(
        &self,
        timeline: &WorkflowTimeline,
        pattern: &WorkflowPattern,
    ) -> Option<EfficiencyRecommendation> {
        // Check if timeline shows signs of the failure pattern
        if pattern.pattern_type == PatternType::Failure {
            if let Some(criteria) = pattern.criteria.as_object() {
                if let Some(min_clarifications) = criteria.get("min_clarification_requests").and_then(|v| v.as_i64()) {
                    let clarification_count = timeline.key_events.iter()
                        .filter(|e| e.event_type.contains("request_clarification"))
                        .count();

                    if clarification_count >= min_clarifications as usize {
                        return Some(EfficiencyRecommendation {
                            id: Uuid::new_v4(),
                            organization_id: timeline.organization_id.clone(),
                            target_type: RecommendationTarget::Task,
                            target_id: Some(timeline.entity_id.clone()),
                            recommendation_type: RecommendationType::Process,
                            title: "High clarification count detected".to_string(),
                            description: format!(
                                "This task has {} clarification requests, which is associated with higher failure rates. Consider pausing to align on requirements.",
                                clarification_count
                            ),
                            suggested_actions: vec![
                                "Schedule a requirements clarification meeting".to_string(),
                                "Document all open questions".to_string(),
                                "Get explicit sign-off on requirements before proceeding".to_string(),
                            ],
                            based_on_patterns: vec![pattern.id.to_string()],
                            evidence: serde_json::json!({
                                "clarification_count": clarification_count,
                                "pattern_threshold": min_clarifications,
                            }),
                            priority: UrgencyLevel::High,
                            estimated_time_savings_hours: pattern.avg_time_impact_hours,
                            status: RecommendationStatus::Pending,
                            user_feedback: None,
                            generated_at: Utc::now(),
                        });
                    }
                }
            }
        }

        None
    }

    /// Generate recommendation for a specific bottleneck
    fn generate_bottleneck_recommendation(
        &self,
        timeline: &WorkflowTimeline,
        bottleneck: &WorkflowBottleneck,
    ) -> EfficiencyRecommendation {
        let (rec_type, title, actions) = match bottleneck.bottleneck_type.as_str() {
            "approval_delay" => (
                RecommendationType::Process,
                "Speed up approval process".to_string(),
                vec![
                    "Set up automatic approval reminders".to_string(),
                    "Consider delegating approval authority".to_string(),
                    "Batch similar approvals together".to_string(),
                ],
            ),
            "blocked_period" => (
                RecommendationType::Resource,
                "Resolve blocking dependency".to_string(),
                vec![
                    "Identify and escalate blocking issues early".to_string(),
                    "Have backup tasks for blocked periods".to_string(),
                    "Create a dependency tracking system".to_string(),
                ],
            ),
            "communication_gap" => (
                RecommendationType::Communication,
                "Address communication gap".to_string(),
                vec![
                    "Set up daily check-in reminders".to_string(),
                    "Assign a point person for updates".to_string(),
                    "Use automated status polling".to_string(),
                ],
            ),
            "clarification_loop" => (
                RecommendationType::Process,
                "Improve requirement clarity".to_string(),
                vec![
                    "Create detailed specification template".to_string(),
                    "Hold kickoff meeting with all stakeholders".to_string(),
                    "Document acceptance criteria upfront".to_string(),
                ],
            ),
            _ => (
                RecommendationType::Process,
                format!("Address {} bottleneck", bottleneck.bottleneck_type),
                vec![
                    "Analyze root cause".to_string(),
                    "Document lessons learned".to_string(),
                ],
            ),
        };

        EfficiencyRecommendation {
            id: Uuid::new_v4(),
            organization_id: timeline.organization_id.clone(),
            target_type: RecommendationTarget::Task,
            target_id: Some(timeline.entity_id.clone()),
            recommendation_type: rec_type,
            title,
            description: format!(
                "{} (duration: {:.1}h)",
                bottleneck.description,
                bottleneck.duration_hours
            ),
            suggested_actions: actions,
            based_on_patterns: vec![],
            evidence: serde_json::json!({
                "bottleneck_type": bottleneck.bottleneck_type,
                "duration_hours": bottleneck.duration_hours,
                "start": bottleneck.start.to_rfc3339(),
                "end": bottleneck.end.to_rfc3339(),
            }),
            priority: if bottleneck.duration_hours > 24.0 {
                UrgencyLevel::High
            } else if bottleneck.duration_hours > 8.0 {
                UrgencyLevel::Medium
            } else {
                UrgencyLevel::Low
            },
            estimated_time_savings_hours: Some(bottleneck.duration_hours * 0.5),
            status: RecommendationStatus::Pending,
            user_feedback: None,
            generated_at: Utc::now(),
        }
    }

    /// Generate actions for a bottleneck pattern
    fn generate_bottleneck_actions(&self, pattern_name: &str) -> Vec<String> {
        match pattern_name {
            "common_approval_delay" => vec![
                "Review approval workflow for bottlenecks".to_string(),
                "Consider parallel approval paths".to_string(),
                "Set SLA for approvals".to_string(),
            ],
            "common_blocked_period" => vec![
                "Improve dependency identification in planning".to_string(),
                "Create escalation paths for blockers".to_string(),
                "Maintain a dependencies register".to_string(),
            ],
            "common_communication_gap" => vec![
                "Standardize update frequency".to_string(),
                "Implement automated progress tracking".to_string(),
                "Set up async communication norms".to_string(),
            ],
            _ => vec![
                "Analyze pattern root cause".to_string(),
                "Create preventive measures".to_string(),
                "Monitor for recurrence".to_string(),
            ],
        }
    }

    /// Check for clarification overload
    fn check_clarification_overload(&self, timeline: &WorkflowTimeline) -> Option<EfficiencyRecommendation> {
        let clarification_count = timeline.key_events.iter()
            .filter(|e| e.event_type.contains("request_clarification"))
            .count();

        if clarification_count >= 3 {
            Some(EfficiencyRecommendation {
                id: Uuid::new_v4(),
                organization_id: timeline.organization_id.clone(),
                target_type: RecommendationTarget::Task,
                target_id: Some(timeline.entity_id.clone()),
                recommendation_type: RecommendationType::Process,
                title: "Multiple clarification requests detected".to_string(),
                description: format!(
                    "This task has {} clarification requests. Consider improving initial specification quality.",
                    clarification_count
                ),
                suggested_actions: vec![
                    "Pause and consolidate all open questions".to_string(),
                    "Schedule alignment meeting with stakeholders".to_string(),
                    "Update task description with clarified requirements".to_string(),
                ],
                based_on_patterns: vec![],
                evidence: serde_json::json!({
                    "clarification_count": clarification_count,
                }),
                priority: UrgencyLevel::Medium,
                estimated_time_savings_hours: Some((clarification_count as f64) * 0.5),
                status: RecommendationStatus::Pending,
                user_feedback: None,
                generated_at: Utc::now(),
            })
        } else {
            None
        }
    }

    /// Check for approval process issues
    fn check_approval_process(&self, timeline: &WorkflowTimeline) -> Option<EfficiencyRecommendation> {
        let approval_requests = timeline.key_events.iter()
            .filter(|e| e.event_type.contains("request_approval"))
            .count();

        let approvals_received = timeline.key_events.iter()
            .filter(|e| e.event_type.contains("approved"))
            .count();

        if approval_requests > 0 && approvals_received == 0 && timeline.status != "completed" {
            Some(EfficiencyRecommendation {
                id: Uuid::new_v4(),
                organization_id: timeline.organization_id.clone(),
                target_type: RecommendationTarget::Task,
                target_id: Some(timeline.entity_id.clone()),
                recommendation_type: RecommendationType::Process,
                title: "Pending approval detected".to_string(),
                description: format!(
                    "Task has {} approval request(s) pending. Consider following up.",
                    approval_requests
                ),
                suggested_actions: vec![
                    "Send reminder to approver".to_string(),
                    "Clarify approval criteria if unclear".to_string(),
                    "Consider escalation if blocking progress".to_string(),
                ],
                based_on_patterns: vec![],
                evidence: serde_json::json!({
                    "pending_approvals": approval_requests,
                }),
                priority: UrgencyLevel::High,
                estimated_time_savings_hours: None,
                status: RecommendationStatus::Pending,
                user_feedback: None,
                generated_at: Utc::now(),
            })
        } else {
            None
        }
    }

    /// Remove duplicate or very similar recommendations
    fn deduplicate_recommendations(
        &self,
        recommendations: Vec<EfficiencyRecommendation>,
    ) -> Vec<EfficiencyRecommendation> {
        let mut unique = Vec::new();
        let mut seen_titles = std::collections::HashSet::new();

        for rec in recommendations {
            // Simple deduplication by title similarity
            let key = rec.title.to_lowercase();
            if !seen_titles.contains(&key) {
                seen_titles.insert(key);
                unique.push(rec);
            }
        }

        unique
    }
}

impl Default for Recommender {
    fn default() -> Self {
        Self::basic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bottleneck_recommendation_priority() {
        let recommender = Recommender::basic();

        // Create a mock timeline
        let timeline = WorkflowTimeline {
            id: Uuid::new_v4(),
            organization_id: "test-org".to_string(),
            entity_type: "task".to_string(),
            entity_id: "task-123".to_string(),
            total_interactions: 5,
            total_participants: 2,
            total_duration_hours: Some(48.0),
            phases: vec![],
            key_events: vec![],
            bottlenecks: vec![],
            status: "in_progress".to_string(),
            opened_at: Utc::now(),
            closed_at: None,
            last_analyzed_at: Utc::now(),
        };

        // Test high priority for long bottleneck
        let long_bottleneck = WorkflowBottleneck {
            bottleneck_type: "approval_delay".to_string(),
            duration_hours: 48.0,
            description: "Long approval delay".to_string(),
            start: Utc::now(),
            end: Utc::now(),
            caused_by: None,
        };

        let rec = recommender.generate_bottleneck_recommendation(&timeline, &long_bottleneck);
        assert_eq!(rec.priority, UrgencyLevel::High);

        // Test medium priority for moderate bottleneck
        let medium_bottleneck = WorkflowBottleneck {
            bottleneck_type: "approval_delay".to_string(),
            duration_hours: 16.0,
            description: "Moderate approval delay".to_string(),
            start: Utc::now(),
            end: Utc::now(),
            caused_by: None,
        };

        let rec = recommender.generate_bottleneck_recommendation(&timeline, &medium_bottleneck);
        assert_eq!(rec.priority, UrgencyLevel::Medium);
    }
}
