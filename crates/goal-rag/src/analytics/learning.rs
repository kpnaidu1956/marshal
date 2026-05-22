//! Intervention tracking and outcome learning
//!
//! Tracks when recommendations are acted upon and measures outcomes.
//! Uses outcome data to improve recommendation confidence over time.

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::aggregation_types::*;
use super::storage::AnalyticsDb;
use crate::error::Result;

/// Learning system for tracking interventions and adjusting confidence
pub struct LearningSystem<'a> {
    db: &'a AnalyticsDb,
}

impl<'a> LearningSystem<'a> {
    pub fn new(db: &'a AnalyticsDb) -> Self {
        Self { db }
    }

    /// Record that a recommendation was acted upon
    pub fn record_intervention(
        &self,
        organization_id: &str,
        recommendation_id: Uuid,
        intervention_type: &str,
        pre_metrics: Option<serde_json::Value>,
    ) -> Result<InterventionOutcome> {
        let outcome = InterventionOutcome {
            id: Uuid::new_v4(),
            organization_id: organization_id.to_string(),
            recommendation_id,
            intervention_type: intervention_type.to_string(),
            intervention_date: Utc::now(),
            outcome_measured_date: None,
            outcome_type: None,
            outcome_value: None,
            pre_intervention_metrics: pre_metrics,
            post_intervention_metrics: None,
            confidence_score: 0.0, // Will be set when outcome is measured
            learned_pattern_id: None,
            created_at: Utc::now(),
        };

        self.db.insert_intervention_outcome(&outcome)?;
        Ok(outcome)
    }

    /// Record the measured outcome of an intervention
    pub fn record_outcome(
        &self,
        intervention_id: &Uuid,
        outcome_type: &str,
        outcome_value: f64,
        post_metrics: &serde_json::Value,
    ) -> Result<bool> {
        self.db.update_intervention_outcome(
            intervention_id,
            outcome_type,
            outcome_value,
            post_metrics,
        )
    }

    /// Analyze intervention effectiveness and learn
    pub fn analyze_effectiveness(
        &self,
        _organization_id: &str,
        recommendation_id: &Uuid,
    ) -> Result<EffectivenessAnalysis> {
        let outcomes = self.db.get_intervention_outcomes(recommendation_id)?;

        if outcomes.is_empty() {
            return Ok(EffectivenessAnalysis {
                recommendation_id: *recommendation_id,
                total_interventions: 0,
                measured_outcomes: 0,
                positive_outcomes: 0,
                negative_outcomes: 0,
                neutral_outcomes: 0,
                avg_improvement: 0.0,
                confidence: 0.0,
                recommendation: ConfidenceRecommendation::InsufficientData,
            });
        }

        let total = outcomes.len() as u32;
        let measured: Vec<_> = outcomes
            .iter()
            .filter(|o| o.outcome_value.is_some())
            .collect();

        let measured_count = measured.len() as u32;

        if measured_count == 0 {
            return Ok(EffectivenessAnalysis {
                recommendation_id: *recommendation_id,
                total_interventions: total,
                measured_outcomes: 0,
                positive_outcomes: 0,
                negative_outcomes: 0,
                neutral_outcomes: 0,
                avg_improvement: 0.0,
                confidence: 0.0,
                recommendation: ConfidenceRecommendation::AwaitingMeasurement,
            });
        }

        // Positive = outcome_value < 0 (reduction in bottlenecks, delays, etc.)
        // Negative = outcome_value > 0.1 (increase in issues)
        // Neutral = between -0.1 and 0.1
        let positive = measured
            .iter()
            .filter(|o| o.outcome_value.map(|v| v < -0.1).unwrap_or(false))
            .count() as u32;
        let negative = measured
            .iter()
            .filter(|o| o.outcome_value.map(|v| v > 0.1).unwrap_or(false))
            .count() as u32;
        let neutral = measured_count - positive - negative;

        let avg_improvement = if measured_count > 0 {
            measured
                .iter()
                .filter_map(|o| o.outcome_value)
                .sum::<f64>()
                / measured_count as f64
        } else {
            0.0
        };

        // Calculate confidence based on outcomes
        let confidence = if measured_count >= 5 {
            let success_rate = positive as f32 / measured_count as f32;
            // Confidence increases with success rate and sample size
            (success_rate * 0.7 + (measured_count as f32 / 10.0).min(0.3)).min(1.0)
        } else {
            // Low confidence with few samples
            0.3 * (measured_count as f32 / 5.0)
        };

        let recommendation = if measured_count < 3 {
            ConfidenceRecommendation::InsufficientData
        } else if positive as f32 / measured_count as f32 >= 0.7 {
            ConfidenceRecommendation::IncreaseConfidence
        } else if negative as f32 / measured_count as f32 >= 0.5 {
            ConfidenceRecommendation::DecreaseConfidence
        } else {
            ConfidenceRecommendation::MaintainConfidence
        };

        Ok(EffectivenessAnalysis {
            recommendation_id: *recommendation_id,
            total_interventions: total,
            measured_outcomes: measured_count,
            positive_outcomes: positive,
            negative_outcomes: negative,
            neutral_outcomes: neutral,
            avg_improvement,
            confidence,
            recommendation,
        })
    }

    /// Get overall learning effectiveness for an organization
    pub fn get_organization_effectiveness(
        &self,
        organization_id: &str,
    ) -> Result<OrganizationLearning> {
        // Get all recommendations with their statuses
        let recommendations = self.db.get_recommendations_for_organization(organization_id, 1000)?;

        let total_recommendations = recommendations.len() as u32;
        let implemented = recommendations
            .iter()
            .filter(|r| r.status.as_str() == "implemented")
            .count() as u32;
        let accepted = recommendations
            .iter()
            .filter(|r| r.status.as_str() == "accepted")
            .count() as u32;
        let rejected = recommendations
            .iter()
            .filter(|r| r.status.as_str() == "rejected")
            .count() as u32;

        // Calculate effectiveness metrics
        let adoption_rate = if total_recommendations > 0 {
            (implemented + accepted) as f32 / total_recommendations as f32
        } else {
            0.0
        };

        // Get patterns to assess learning effectiveness
        let patterns = self.db.get_active_patterns(organization_id)?;
        let high_confidence_patterns = patterns
            .iter()
            .filter(|p| p.confidence_score >= 0.7)
            .count() as u32;

        Ok(OrganizationLearning {
            organization_id: organization_id.to_string(),
            total_recommendations,
            implemented_count: implemented,
            accepted_count: accepted,
            rejected_count: rejected,
            adoption_rate,
            total_patterns_learned: patterns.len() as u32,
            high_confidence_patterns,
            learning_velocity: self.calculate_learning_velocity(organization_id)?,
            computed_at: Utc::now(),
        })
    }

    /// Apply learned adjustments to pattern confidence
    /// Optimized to batch-fetch recommendations for all patterns to avoid O(n²) queries
    pub fn apply_learning_adjustments(&self, organization_id: &str) -> Result<LearningAdjustmentResult> {
        let patterns = self.db.get_active_patterns(organization_id)?;
        let mut patterns_adjusted = 0;
        let mut confidence_increased = 0;
        let mut confidence_decreased = 0;

        // Early return if no patterns
        if patterns.is_empty() {
            return Ok(LearningAdjustmentResult {
                patterns_analyzed: 0,
                patterns_adjusted: 0,
                confidence_increased: 0,
                confidence_decreased: 0,
            });
        }

        // Pre-fetch all recommendations for the organization (single query)
        // This avoids N+1 query problem when iterating through patterns
        let all_recommendations = self.db.get_recommendations_for_organization(organization_id, 10000)?;

        // Build a map of pattern_id -> recommendations
        let mut pattern_recommendations: std::collections::HashMap<Uuid, Vec<&crate::analytics::types::EfficiencyRecommendation>> =
            std::collections::HashMap::new();

        for rec in &all_recommendations {
            // Parse the based_on_patterns JSON array to find pattern IDs
            for pattern in &patterns {
                let pattern_id_str = pattern.id.to_string();
                // Check if this recommendation references the pattern
                let patterns_json = serde_json::to_string(&rec.based_on_patterns).unwrap_or_default();
                if patterns_json.contains(&pattern_id_str) {
                    pattern_recommendations.entry(pattern.id)
                        .or_default()
                        .push(rec);
                }
            }
        }

        for pattern in &patterns {
            let recommendations = pattern_recommendations.get(&pattern.id);

            let recommendations = match recommendations {
                Some(recs) if !recs.is_empty() => recs,
                _ => continue,
            };

            // Calculate effectiveness across all recommendations from this pattern
            let mut total_positive = 0u32;
            let mut total_measured = 0u32;

            for rec in recommendations {
                let analysis = self.analyze_effectiveness(organization_id, &rec.id)?;
                total_positive += analysis.positive_outcomes;
                total_measured += analysis.measured_outcomes;
            }

            if total_measured < 3 {
                continue; // Not enough data to adjust
            }

            let current_confidence = pattern.confidence_score;
            let success_rate = total_positive as f32 / total_measured as f32;

            let new_confidence = if success_rate >= 0.7 {
                // Good success rate - increase confidence
                (current_confidence + 0.1).min(0.95)
            } else if success_rate <= 0.3 {
                // Poor success rate - decrease confidence
                (current_confidence - 0.15).max(0.1)
            } else {
                // Moderate success - slight adjustment toward 0.5
                current_confidence + (0.5 - current_confidence) * 0.1
            };

            if (new_confidence - current_confidence).abs() > 0.01 {
                self.db.update_pattern_confidence(&pattern.id, new_confidence)?;
                patterns_adjusted += 1;

                if new_confidence > current_confidence {
                    confidence_increased += 1;
                } else {
                    confidence_decreased += 1;
                }
            }
        }

        Ok(LearningAdjustmentResult {
            patterns_analyzed: patterns.len() as u32,
            patterns_adjusted,
            confidence_increased,
            confidence_decreased,
        })
    }

    /// Calculate learning velocity (rate of pattern discovery and validation)
    fn calculate_learning_velocity(&self, organization_id: &str) -> Result<f32> {
        let patterns = self.db.get_active_patterns(organization_id)?;

        if patterns.is_empty() {
            return Ok(0.0);
        }

        // Calculate patterns discovered in last 30 days
        let thirty_days_ago = Utc::now() - Duration::days(30);
        let recent_patterns = patterns
            .iter()
            .filter(|p| p.created_at >= thirty_days_ago)
            .count();

        // Calculate patterns validated (confidence > 0.6) in last 30 days
        let validated_patterns = patterns
            .iter()
            .filter(|p| p.updated_at >= thirty_days_ago && p.confidence_score >= 0.6)
            .count();

        // Velocity = (recent patterns + 2 * validated patterns) / 30 days
        let velocity = (recent_patterns + 2 * validated_patterns) as f32 / 30.0;

        Ok(velocity)
    }

    /// Get interventions awaiting outcome measurement
    pub fn get_pending_outcome_measurements(
        &self,
        organization_id: &str,
        days_since_intervention: i64,
    ) -> Result<Vec<InterventionOutcome>> {
        let cutoff = Utc::now() - Duration::days(days_since_intervention);

        // Get all outcomes that don't have measured results yet
        // and are older than the specified days
        let all_recommendations = self.db.get_recommendations_for_organization(organization_id, 1000)?;

        let mut pending = Vec::new();
        for rec in all_recommendations {
            if rec.status.as_str() == "implemented" || rec.status.as_str() == "accepted" {
                let outcomes = self.db.get_intervention_outcomes(&rec.id)?;
                for outcome in outcomes {
                    if outcome.outcome_measured_date.is_none()
                        && outcome.intervention_date <= cutoff
                    {
                        pending.push(outcome);
                    }
                }
            }
        }

        Ok(pending)
    }
}

/// Analysis of intervention effectiveness
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EffectivenessAnalysis {
    pub recommendation_id: Uuid,
    pub total_interventions: u32,
    pub measured_outcomes: u32,
    pub positive_outcomes: u32,
    pub negative_outcomes: u32,
    pub neutral_outcomes: u32,
    pub avg_improvement: f64,
    pub confidence: f32,
    pub recommendation: ConfidenceRecommendation,
}

/// Confidence adjustment recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConfidenceRecommendation {
    IncreaseConfidence,
    DecreaseConfidence,
    MaintainConfidence,
    InsufficientData,
    AwaitingMeasurement,
}

/// Organization-level learning metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrganizationLearning {
    pub organization_id: String,
    pub total_recommendations: u32,
    pub implemented_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub adoption_rate: f32,
    pub total_patterns_learned: u32,
    pub high_confidence_patterns: u32,
    pub learning_velocity: f32,
    pub computed_at: DateTime<Utc>,
}

/// Result of applying learning adjustments
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LearningAdjustmentResult {
    pub patterns_analyzed: u32,
    pub patterns_adjusted: u32,
    pub confidence_increased: u32,
    pub confidence_decreased: u32,
}
