//! Aggregation computation engine
//!
//! Computes interaction type, sentiment, and bottleneck aggregations
//! at team and organization levels for daily, weekly, and monthly periods.

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use super::aggregation_types::*;
use super::storage::AnalyticsDb;
use super::types::{InteractionClassification, WorkflowTimeline};
use crate::error::Result;

/// Aggregation engine for computing team and org-level metrics
pub struct Aggregator<'a> {
    db: &'a AnalyticsDb,
}

impl<'a> Aggregator<'a> {
    pub fn new(db: &'a AnalyticsDb) -> Self {
        Self { db }
    }

    /// Compute interaction type aggregations for a period
    /// When member_ids is Some, filters to only those users (avoids re-querying team members)
    pub fn compute_interaction_type_aggregation(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
        classifications: &[InteractionClassification],
        member_ids: Option<&[String]>,
    ) -> Result<InteractionTypeAggregation> {
        // Filter by team if specified
        let filtered: Vec<_> = if let Some(ids) = member_ids {
            classifications
                .iter()
                .filter(|c| ids.iter().any(|id| id == &c.sender_id))
                .collect()
        } else if let Some(tid) = team_id {
            let members = self.db.get_team_members(organization_id, tid)?;
            let ids: Vec<_> = members.iter().map(|m| m.user_id.as_str()).collect();
            classifications
                .iter()
                .filter(|c| ids.contains(&c.sender_id.as_str()))
                .collect()
        } else {
            classifications.iter().collect()
        };

        // Count by interaction type
        let mut type_counts: HashMap<String, u32> = HashMap::new();
        for c in &filtered {
            *type_counts.entry(c.interaction_type.as_str().to_string()).or_insert(0) += 1;
        }

        let total = filtered.len() as u32;
        let clarification_count = type_counts.get("request_clarification").copied().unwrap_or(0);
        let blocker_count = type_counts.get("blocker").copied().unwrap_or(0);
        let escalation_count = type_counts.get("escalation").copied().unwrap_or(0);

        let clarification_ratio = if total > 0 { clarification_count as f32 / total as f32 } else { 0.0 };
        let blocker_ratio = if total > 0 { blocker_count as f32 / total as f32 } else { 0.0 };
        let escalation_ratio = if total > 0 { escalation_count as f32 / total as f32 } else { 0.0 };

        Ok(InteractionTypeAggregation {
            id: Uuid::new_v4(),
            organization_id: organization_id.to_string(),
            team_id: team_id.map(|s| s.to_string()),
            period_start,
            period_end,
            period_type,
            type_counts,
            total_interactions: total,
            clarification_ratio,
            blocker_ratio,
            escalation_ratio,
            computed_at: Utc::now(),
        })
    }

    /// Compute sentiment aggregation for a period
    /// When member_ids is Some, filters to only those users (avoids re-querying team members)
    /// When rolling_classifications is Some, uses it for rolling averages (avoids re-querying)
    pub fn compute_sentiment_aggregation(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
        classifications: &[InteractionClassification],
        member_ids: Option<&[String]>,
        rolling_classifications: Option<&[InteractionClassification]>,
    ) -> Result<SentimentAggregation> {
        // Filter by team if specified
        let filtered: Vec<_> = if let Some(ids) = member_ids {
            classifications
                .iter()
                .filter(|c| ids.iter().any(|id| id == &c.sender_id))
                .collect()
        } else if let Some(tid) = team_id {
            let members = self.db.get_team_members(organization_id, tid)?;
            let ids: Vec<_> = members.iter().map(|m| m.user_id.as_str()).collect();
            classifications
                .iter()
                .filter(|c| ids.contains(&c.sender_id.as_str()))
                .collect()
        } else {
            classifications.iter().collect()
        };

        if filtered.is_empty() {
            return Ok(SentimentAggregation {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                team_id: team_id.map(|s| s.to_string()),
                period_start,
                period_end,
                period_type,
                avg_sentiment: 0.0,
                min_sentiment: 0.0,
                max_sentiment: 0.0,
                sentiment_std_dev: 0.0,
                positive_count: 0,
                neutral_count: 0,
                negative_count: 0,
                sentiment_by_type: HashMap::new(),
                rolling_7day_avg: None,
                rolling_30day_avg: None,
                total_interactions: 0,
                computed_at: Utc::now(),
            });
        }

        let sentiments: Vec<f32> = filtered.iter().map(|c| c.sentiment).collect();
        let sum: f32 = sentiments.iter().sum();
        let avg = sum / sentiments.len() as f32;
        let min = sentiments.iter().cloned().fold(f32::MAX, f32::min);
        let max = sentiments.iter().cloned().fold(f32::MIN, f32::max);

        // Standard deviation
        let variance: f32 = sentiments.iter().map(|s| (s - avg).powi(2)).sum::<f32>() / sentiments.len() as f32;
        let std_dev = variance.sqrt();

        // Count positive/neutral/negative
        let positive_count = sentiments.iter().filter(|&&s| s > 0.3).count() as u32;
        let neutral_count = sentiments.iter().filter(|&&s| (-0.3..=0.3).contains(&s)).count() as u32;
        let negative_count = sentiments.iter().filter(|&&s| s < -0.3).count() as u32;

        // Sentiment by type
        let mut sentiment_by_type: HashMap<String, f32> = HashMap::new();
        let mut type_sums: HashMap<String, (f32, u32)> = HashMap::new();
        for c in &filtered {
            let entry = type_sums.entry(c.interaction_type.as_str().to_string()).or_insert((0.0, 0));
            entry.0 += c.sentiment;
            entry.1 += 1;
        }
        for (t, (sum, count)) in type_sums {
            if count > 0 {
                sentiment_by_type.insert(t, sum / count as f32);
            }
        }

        // Rolling averages (use pre-fetched data if available, otherwise query)
        let rolling_data = rolling_classifications.unwrap_or(classifications);
        let rolling_7day_avg = self.compute_rolling_sentiment_avg(organization_id, member_ids, 7, rolling_data)?;
        let rolling_30day_avg = self.compute_rolling_sentiment_avg(organization_id, member_ids, 30, rolling_data)?;

        Ok(SentimentAggregation {
            id: Uuid::new_v4(),
            organization_id: organization_id.to_string(),
            team_id: team_id.map(|s| s.to_string()),
            period_start,
            period_end,
            period_type,
            avg_sentiment: avg,
            min_sentiment: min,
            max_sentiment: max,
            sentiment_std_dev: std_dev,
            positive_count,
            neutral_count,
            negative_count,
            sentiment_by_type,
            rolling_7day_avg,
            rolling_30day_avg,
            total_interactions: filtered.len() as u32,
            computed_at: Utc::now(),
        })
    }

    /// Compute bottleneck aggregation for a period
    pub fn compute_bottleneck_aggregation(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
        timelines: &[WorkflowTimeline],
    ) -> Result<BottleneckAggregation> {
        // Filter timelines by team (based on participants if team specified)
        let filtered: Vec<_> = if team_id.is_some() {
            // For team filtering, we'd need to check timeline participants
            // For now, include all timelines (team filtering can be refined later)
            timelines.iter().collect()
        } else {
            timelines.iter().collect()
        };

        let mut type_counts: HashMap<String, u32> = HashMap::new();
        let mut type_total_hours: HashMap<String, f64> = HashMap::new();

        for timeline in &filtered {
            for bottleneck in &timeline.bottlenecks {
                let bt = &bottleneck.bottleneck_type;
                *type_counts.entry(bt.clone()).or_insert(0) += 1;
                *type_total_hours.entry(bt.clone()).or_insert(0.0) += bottleneck.duration_hours;
            }
        }

        // Compute averages
        let mut type_avg_hours: HashMap<String, f64> = HashMap::new();
        for (bt, total_hours) in &type_total_hours {
            let count = type_counts.get(bt).copied().unwrap_or(1) as f64;
            type_avg_hours.insert(bt.clone(), total_hours / count);
        }

        let total_bottlenecks: u32 = type_counts.values().sum();
        let total_hours_lost: f64 = type_total_hours.values().sum();
        let avg_duration = if total_bottlenecks > 0 {
            total_hours_lost / total_bottlenecks as f64
        } else {
            0.0
        };

        // Compute trend (compare with previous period)
        let (trend_direction, trend_percent_change) = self.compute_bottleneck_trend(
            organization_id,
            team_id,
            period_type,
            total_bottlenecks,
            &period_start,
        )?;

        Ok(BottleneckAggregation {
            id: Uuid::new_v4(),
            organization_id: organization_id.to_string(),
            team_id: team_id.map(|s| s.to_string()),
            period_start,
            period_end,
            period_type,
            type_counts,
            type_total_hours,
            type_avg_hours,
            total_bottlenecks,
            total_hours_lost,
            avg_bottleneck_duration: avg_duration,
            trend_direction,
            trend_percent_change,
            computed_at: Utc::now(),
        })
    }

    /// Compute aggregations for a specific period and persist
    pub fn run_aggregations(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
    ) -> Result<AggregationResult> {
        // Fetch raw data
        let classifications = self.db.get_classifications_in_range(organization_id, &period_start, &period_end)?;
        let timelines = self.db.get_timelines_for_org(organization_id)?;

        self.run_aggregations_with_data(
            organization_id, team_id, period_start, period_end, period_type,
            &classifications, &timelines, None, None,
        )
    }

    /// Compute aggregations with pre-fetched data (avoids redundant queries)
    fn run_aggregations_with_data(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
        classifications: &[InteractionClassification],
        timelines: &[WorkflowTimeline],
        member_ids: Option<&[String]>,
        rolling_classifications: Option<&[InteractionClassification]>,
    ) -> Result<AggregationResult> {
        let interaction_agg = self.compute_interaction_type_aggregation(
            organization_id, team_id, period_start, period_end, period_type,
            classifications, member_ids,
        )?;

        let sentiment_agg = self.compute_sentiment_aggregation(
            organization_id, team_id, period_start, period_end, period_type,
            classifications, member_ids, rolling_classifications,
        )?;

        let bottleneck_agg = self.compute_bottleneck_aggregation(
            organization_id, team_id, period_start, period_end, period_type, timelines,
        )?;

        // Persist
        self.db.upsert_interaction_type_aggregation(&interaction_agg)?;
        self.db.upsert_sentiment_aggregation(&sentiment_agg)?;
        self.db.upsert_bottleneck_aggregation(&bottleneck_agg)?;

        Ok(AggregationResult {
            interactions_processed: classifications.len() as u32,
            timelines_processed: timelines.len() as u32,
            aggregations_created: 3,
        })
    }

    /// Run aggregations for all teams in an organization
    /// Pre-fetches shared data once to avoid N+1 query patterns
    pub fn run_all_team_aggregations(
        &self,
        organization_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
    ) -> Result<AllTeamsAggregationResult> {
        // Pre-fetch ALL shared data once (biggest perf win: eliminates N+1 queries)
        let rolling_start = Utc::now() - Duration::days(30);
        let widest_start = if period_start < rolling_start { period_start } else { rolling_start };
        let all_classifications = self.db.get_classifications_in_range(organization_id, &widest_start, &period_end)?;
        let timelines = self.db.get_timelines_for_org(organization_id)?;

        // Filter classifications to just the aggregation period
        let period_classifications: Vec<_> = all_classifications
            .iter()
            .filter(|c| c.original_created_at >= period_start && c.original_created_at < period_end)
            .cloned()
            .collect();

        // Get all teams and pre-fetch their member IDs
        let teams = self.db.list_teams(organization_id)?;
        let mut team_members_map: HashMap<String, Vec<String>> = HashMap::new();
        for team in &teams {
            let members = self.db.get_team_members(organization_id, &team.team_id)?;
            team_members_map.insert(
                team.team_id.clone(),
                members.iter().map(|m| m.user_id.clone()).collect(),
            );
        }

        // Org-level aggregation (no team filter)
        let org_result = self.run_aggregations_with_data(
            organization_id, None, period_start, period_end, period_type,
            &period_classifications, &timelines, None, Some(&all_classifications),
        )?;

        // Per-team aggregations with pre-fetched member IDs
        let mut team_results = Vec::new();
        for team in &teams {
            let member_ids = team_members_map.get(&team.team_id);
            let result = self.run_aggregations_with_data(
                organization_id, Some(&team.team_id), period_start, period_end, period_type,
                &period_classifications, &timelines,
                member_ids.map(|v| v.as_slice()),
                Some(&all_classifications),
            )?;
            team_results.push((team.team_id.clone(), result));
        }

        Ok(AllTeamsAggregationResult {
            organization_result: org_result,
            team_results,
            teams_processed: teams.len() as u32,
        })
    }

    /// Compute rolling sentiment average from pre-fetched data
    /// Uses pre-fetched classifications and member_ids to avoid redundant DB queries
    fn compute_rolling_sentiment_avg(
        &self,
        _organization_id: &str,
        member_ids: Option<&[String]>,
        days: i64,
        all_classifications: &[InteractionClassification],
    ) -> Result<Option<f32>> {
        let end = Utc::now();
        let start = end - Duration::days(days);

        // Filter by date range from pre-fetched data
        let in_range: Vec<_> = all_classifications
            .iter()
            .filter(|c| c.original_created_at >= start && c.original_created_at < end)
            .collect();

        // Filter by team members if specified
        let filtered: Vec<_> = if let Some(ids) = member_ids {
            in_range.into_iter().filter(|c| ids.iter().any(|id| id == &c.sender_id)).collect()
        } else {
            in_range
        };

        if filtered.is_empty() {
            return Ok(None);
        }

        let sum: f32 = filtered.iter().map(|c| c.sentiment).sum();
        Ok(Some(sum / filtered.len() as f32))
    }

    /// Compute trend direction by comparing with previous period
    fn compute_bottleneck_trend(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_type: PeriodType,
        current_count: u32,
        period_start: &DateTime<Utc>,
    ) -> Result<(TrendDirection, f32)> {
        // Calculate previous period start based on period_type
        let period_duration = match period_type {
            PeriodType::Daily => Duration::days(1),
            PeriodType::Weekly => Duration::days(7),
            PeriodType::Monthly => Duration::days(30),
        };

        let previous_period_start = *period_start - period_duration;

        // Query previous period's bottleneck aggregation
        let previous_agg = self.db.get_bottleneck_aggregation(
            organization_id,
            team_id,
            &previous_period_start,
            period_type.as_str(),
        )?;

        match previous_agg {
            Some(prev) => {
                let previous_count = prev.total_bottlenecks;

                // Avoid division by zero
                if previous_count == 0 && current_count == 0 {
                    return Ok((TrendDirection::Stable, 0.0));
                }

                // Calculate percent change
                let percent_change = if previous_count == 0 {
                    // Went from 0 to some - 100% increase per bottleneck
                    100.0 * current_count as f32
                } else {
                    let change = current_count as f32 - previous_count as f32;
                    (change / previous_count as f32) * 100.0
                };

                // Determine trend direction with 5% threshold for stability
                let direction = if percent_change < -5.0 {
                    TrendDirection::Improving
                } else if percent_change > 5.0 {
                    TrendDirection::Worsening
                } else {
                    TrendDirection::Stable
                };

                Ok((direction, percent_change))
            }
            None => {
                // No previous data - consider stable
                Ok((TrendDirection::Stable, 0.0))
            }
        }
    }
}

/// Result of a single aggregation run
#[derive(Debug, Clone)]
pub struct AggregationResult {
    pub interactions_processed: u32,
    pub timelines_processed: u32,
    pub aggregations_created: u32,
}

/// Result of running aggregations for all teams
#[derive(Debug, Clone)]
pub struct AllTeamsAggregationResult {
    pub organization_result: AggregationResult,
    pub team_results: Vec<(String, AggregationResult)>,
    pub teams_processed: u32,
}

// Period calculation helpers

/// Get the start of a daily period
pub fn get_daily_period_start(date: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .unwrap_or(date)
}

/// Get the start of a weekly period (Monday)
pub fn get_weekly_period_start(date: DateTime<Utc>) -> DateTime<Utc> {
    let days_from_monday = date.weekday().num_days_from_monday();
    let monday = date - Duration::days(days_from_monday as i64);
    get_daily_period_start(monday)
}

/// Get the start of a monthly period
pub fn get_monthly_period_start(date: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(date.year(), date.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(date)
}

/// Get the end of a period based on type
pub fn get_period_end(start: DateTime<Utc>, period_type: PeriodType) -> DateTime<Utc> {
    match period_type {
        PeriodType::Daily => start + Duration::days(1),
        PeriodType::Weekly => start + Duration::days(7),
        PeriodType::Monthly => {
            let next_month = if start.month() == 12 {
                Utc.with_ymd_and_hms(start.year() + 1, 1, 1, 0, 0, 0)
            } else {
                Utc.with_ymd_and_hms(start.year(), start.month() + 1, 1, 0, 0, 0)
            };
            next_month.single().unwrap_or(start + Duration::days(30))
        }
    }
}
