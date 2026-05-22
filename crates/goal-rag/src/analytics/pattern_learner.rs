//! Pattern learning engine
//!
//! Learns workflow patterns from completed tasks/goals to identify
//! success factors, failure patterns, and efficiency opportunities.

use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

use super::types::*;

/// Pattern learner for identifying workflow patterns
#[allow(dead_code)]
pub struct PatternLearner {
    /// Minimum occurrences to consider a pattern
    min_occurrences: u32,
    /// Minimum confidence to save a pattern (for future use)
    min_confidence: f32,
}

impl PatternLearner {
    pub fn new(min_occurrences: u32, min_confidence: f32) -> Self {
        Self {
            min_occurrences,
            min_confidence,
        }
    }

    pub fn default_settings() -> Self {
        Self::new(3, 0.6)
    }

    /// Learn patterns from a set of workflow timelines
    pub fn learn_patterns(
        &self,
        organization_id: &str,
        timelines: &[WorkflowTimeline],
    ) -> Vec<WorkflowPattern> {
        let mut patterns = Vec::new();

        // Learn success patterns
        patterns.extend(self.learn_success_patterns(organization_id, timelines));

        // Learn failure patterns
        patterns.extend(self.learn_failure_patterns(organization_id, timelines));

        // Learn bottleneck patterns
        patterns.extend(self.learn_bottleneck_patterns(organization_id, timelines));

        // Learn efficiency patterns
        patterns.extend(self.learn_efficiency_patterns(organization_id, timelines));

        patterns
    }

    /// Learn patterns associated with successful completions
    fn learn_success_patterns(
        &self,
        organization_id: &str,
        timelines: &[WorkflowTimeline],
    ) -> Vec<WorkflowPattern> {
        let mut patterns = Vec::new();

        let successful: Vec<_> = timelines.iter()
            .filter(|t| t.status == "completed" || t.status == "done")
            .collect();

        if successful.len() < self.min_occurrences as usize {
            return patterns;
        }

        // Pattern: Quick completion (under median time)
        let durations: Vec<f64> = successful.iter()
            .filter_map(|t| t.total_duration_hours)
            .collect();

        if durations.len() >= self.min_occurrences as usize {
            let median = median(&durations);
            let fast_completions: Vec<&WorkflowTimeline> = successful.iter()
                .filter(|t| t.total_duration_hours.map(|d| d < median).unwrap_or(false))
                .copied()
                .collect();

            if fast_completions.len() >= self.min_occurrences as usize {
                // Analyze what fast completions have in common
                let common_traits = self.analyze_common_traits(&fast_completions);

                if !common_traits.is_empty() {
                    patterns.push(WorkflowPattern {
                        id: Uuid::new_v4(),
                        organization_id: organization_id.to_string(),
                        pattern_type: PatternType::Success,
                        pattern_name: "fast_completion".to_string(),
                        description: format!(
                            "Tasks completed faster than median ({:.1}h) share these traits: {}",
                            median,
                            common_traits.join(", ")
                        ),
                        criteria: serde_json::json!({
                            "duration_below_hours": median,
                            "traits": common_traits,
                        }),
                        occurrence_count: fast_completions.len() as u32,
                        success_correlation: Some(1.0),
                        avg_time_impact_hours: Some(median / 2.0),
                        confidence_score: (fast_completions.len() as f32) / (successful.len() as f32),
                        examples: fast_completions.iter().map(|t| t.entity_id.clone()).collect(),
                        is_active: true,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    });
                }
            }
        }

        // Pattern: Minimal blockers
        let no_bottleneck_completions: Vec<_> = successful.iter()
            .filter(|t| t.bottlenecks.is_empty())
            .collect();

        if no_bottleneck_completions.len() >= self.min_occurrences as usize {
            patterns.push(WorkflowPattern {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                pattern_type: PatternType::Success,
                pattern_name: "smooth_workflow".to_string(),
                description: "Tasks completed without any detected bottlenecks".to_string(),
                criteria: serde_json::json!({
                    "bottleneck_count": 0,
                }),
                occurrence_count: no_bottleneck_completions.len() as u32,
                success_correlation: Some(1.0),
                avg_time_impact_hours: None,
                confidence_score: (no_bottleneck_completions.len() as f32) / (successful.len() as f32),
                examples: no_bottleneck_completions.iter().map(|t| t.entity_id.clone()).collect(),
                is_active: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }

        patterns
    }

    /// Learn patterns associated with failed or stalled tasks
    fn learn_failure_patterns(
        &self,
        organization_id: &str,
        timelines: &[WorkflowTimeline],
    ) -> Vec<WorkflowPattern> {
        let mut patterns = Vec::new();

        let failed: Vec<_> = timelines.iter()
            .filter(|t| t.status == "failed" || t.status == "cancelled" || t.status == "stalled")
            .collect();

        if failed.len() < self.min_occurrences as usize {
            return patterns;
        }

        // Pattern: High clarification count before failure
        let high_clarification: Vec<_> = failed.iter()
            .filter(|t| {
                t.key_events.iter()
                    .filter(|e| e.event_type.contains("request_clarification"))
                    .count() >= 3
            })
            .collect();

        if high_clarification.len() >= self.min_occurrences as usize {
            patterns.push(WorkflowPattern {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                pattern_type: PatternType::Failure,
                pattern_name: "unclear_requirements_failure".to_string(),
                description: "Tasks with 3+ clarification requests have higher failure rates".to_string(),
                criteria: serde_json::json!({
                    "min_clarification_requests": 3,
                }),
                occurrence_count: high_clarification.len() as u32,
                success_correlation: Some(0.0),
                avg_time_impact_hours: None,
                confidence_score: (high_clarification.len() as f32) / (failed.len() as f32),
                examples: high_clarification.iter().map(|t| t.entity_id.clone()).collect(),
                is_active: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }

        patterns
    }

    /// Learn bottleneck patterns
    fn learn_bottleneck_patterns(
        &self,
        organization_id: &str,
        timelines: &[WorkflowTimeline],
    ) -> Vec<WorkflowPattern> {
        let mut patterns = Vec::new();

        // Guard against empty timelines
        if timelines.is_empty() {
            return patterns;
        }

        // Count bottleneck types across all timelines
        let mut bottleneck_counts: HashMap<String, Vec<&WorkflowTimeline>> = HashMap::new();

        for timeline in timelines {
            for bottleneck in &timeline.bottlenecks {
                bottleneck_counts
                    .entry(bottleneck.bottleneck_type.clone())
                    .or_default()
                    .push(timeline);
            }
        }

        for (bottleneck_type, affected_timelines) in bottleneck_counts {
            if affected_timelines.len() >= self.min_occurrences as usize {
                // Count actual bottlenecks of this type for proper averaging
                let matching_bottlenecks: Vec<_> = affected_timelines.iter()
                    .flat_map(|t| t.bottlenecks.iter())
                    .filter(|b| b.bottleneck_type == bottleneck_type)
                    .collect();

                let bottleneck_count = matching_bottlenecks.len();
                let avg_duration: f64 = if bottleneck_count > 0 {
                    matching_bottlenecks.iter().map(|b| b.duration_hours).sum::<f64>()
                        / bottleneck_count as f64
                } else {
                    0.0
                };

                patterns.push(WorkflowPattern {
                    id: Uuid::new_v4(),
                    organization_id: organization_id.to_string(),
                    pattern_type: PatternType::Bottleneck,
                    pattern_name: format!("common_{}", bottleneck_type),
                    description: format!(
                        "Recurring {} bottleneck affecting {} tasks (avg {:.1}h delay)",
                        bottleneck_type,
                        affected_timelines.len(),
                        avg_duration
                    ),
                    criteria: serde_json::json!({
                        "bottleneck_type": bottleneck_type,
                    }),
                    occurrence_count: affected_timelines.len() as u32,
                    success_correlation: None,
                    avg_time_impact_hours: Some(avg_duration),
                    confidence_score: (affected_timelines.len() as f32) / (timelines.len().max(1) as f32),
                    examples: affected_timelines.iter().map(|t| t.entity_id.clone()).collect(),
                    is_active: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                });
            }
        }

        patterns
    }

    /// Learn efficiency patterns
    fn learn_efficiency_patterns(
        &self,
        organization_id: &str,
        timelines: &[WorkflowTimeline],
    ) -> Vec<WorkflowPattern> {
        let mut patterns = Vec::new();

        // Pattern: Optimal team size
        let mut team_size_performance: HashMap<u32, Vec<(&WorkflowTimeline, f64)>> = HashMap::new();

        for timeline in timelines {
            if let Some(duration) = timeline.total_duration_hours {
                team_size_performance
                    .entry(timeline.total_participants)
                    .or_default()
                    .push((timeline, duration));
            }
        }

        // Find optimal team size
        let mut best_team_size = 0u32;
        let mut best_avg_duration = f64::MAX;

        for (team_size, entries) in &team_size_performance {
            if entries.len() >= self.min_occurrences as usize {
                let avg_duration: f64 = entries.iter().map(|(_, d)| d).sum::<f64>() / entries.len() as f64;
                if avg_duration < best_avg_duration {
                    best_avg_duration = avg_duration;
                    best_team_size = *team_size;
                }
            }
        }

        if best_team_size > 0 && team_size_performance.get(&best_team_size).map(|e| e.len()).unwrap_or(0) >= self.min_occurrences as usize {
            let entries = team_size_performance.get(&best_team_size).unwrap();
            patterns.push(WorkflowPattern {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                pattern_type: PatternType::Efficiency,
                pattern_name: "optimal_team_size".to_string(),
                description: format!(
                    "Tasks with {} participants complete fastest (avg {:.1}h)",
                    best_team_size,
                    best_avg_duration
                ),
                criteria: serde_json::json!({
                    "optimal_participants": best_team_size,
                }),
                occurrence_count: entries.len() as u32,
                success_correlation: None,
                avg_time_impact_hours: Some(best_avg_duration),
                confidence_score: (entries.len() as f32) / (timelines.len().max(1) as f32),
                examples: entries.iter().map(|(t, _)| t.entity_id.clone()).collect(),
                is_active: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }

        patterns
    }

    /// Analyze common traits among a set of timelines
    fn analyze_common_traits(&self, timelines: &[&WorkflowTimeline]) -> Vec<String> {
        let mut traits = Vec::new();

        if timelines.is_empty() {
            return traits;
        }

        // Check for low interaction count
        let avg_interactions: f64 = timelines.iter()
            .map(|t| t.total_interactions as f64)
            .sum::<f64>() / timelines.len() as f64;

        if avg_interactions < 5.0 {
            traits.push("minimal communication overhead".to_string());
        }

        // Check for small team size
        let avg_participants: f64 = timelines.iter()
            .map(|t| t.total_participants as f64)
            .sum::<f64>() / timelines.len() as f64;

        if avg_participants <= 2.0 {
            traits.push("focused team (1-2 people)".to_string());
        }

        // Check for no blockers
        let no_blockers = timelines.iter()
            .all(|t| t.bottlenecks.is_empty());

        if no_blockers {
            traits.push("no blocking issues".to_string());
        }

        traits
    }
}

impl Default for PatternLearner {
    fn default() -> Self {
        Self::default_settings()
    }
}

/// Calculate median of a slice
fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_median() {
        assert_eq!(median(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(median(&[5.0]), 5.0);
    }
}
