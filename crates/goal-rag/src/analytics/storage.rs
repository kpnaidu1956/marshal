//! SQLite storage for analytics data
//!
//! Stores interaction classifications, workflow timelines, patterns, and recommendations.
//! Uses r2d2 connection pool for concurrent read access (SQLite WAL mode).

use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

use crate::error::{Error, Result};
use super::types::*;

/// Analytics database for storing classifications, timelines, patterns.
/// Uses a connection pool to allow concurrent reads (SQLite WAL mode).
pub struct AnalyticsDb {
    pool: Pool<SqliteConnectionManager>,
}

impl AnalyticsDb {
    /// Create or open the analytics database with a connection pool
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(8) // Allow up to 8 concurrent readers
            .build(manager)
            .map_err(|e| Error::Internal(format!("Failed to create analytics connection pool: {}", e)))?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Create an in-memory database (for testing)
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(1) // In-memory DBs can't share across connections
            .build(manager)
            .map_err(|e| Error::Internal(format!("Failed to create in-memory pool: {}", e)))?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Get a connection from the pool
    fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get()
            .map_err(|e| Error::Internal(format!("Failed to get analytics DB connection: {}", e)))
    }

    /// Run database migrations
    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;

        // Enable WAL mode for better concurrency
        conn.execute_batch(r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=10000;
            PRAGMA temp_store=MEMORY;
        "#).map_err(|e| Error::Internal(format!("Failed to set pragmas: {}", e)))?;

        conn.execute_batch(r#"
            -- Interaction classifications table
            CREATE TABLE IF NOT EXISTS interaction_classifications (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                source_type TEXT NOT NULL,
                source_id TEXT NOT NULL,
                task_id TEXT,
                goal_id TEXT,
                sender_id TEXT NOT NULL,
                content TEXT NOT NULL,

                interaction_type TEXT NOT NULL,
                secondary_types TEXT,
                confidence_score REAL NOT NULL,
                entities TEXT,
                sentiment REAL,
                urgency_level TEXT,

                references_interaction_id TEXT,
                original_created_at TEXT NOT NULL,
                classified_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_ic_org ON interaction_classifications(organization_id);
            CREATE INDEX IF NOT EXISTS idx_ic_task ON interaction_classifications(task_id);
            CREATE INDEX IF NOT EXISTS idx_ic_goal ON interaction_classifications(goal_id);
            CREATE INDEX IF NOT EXISTS idx_ic_type ON interaction_classifications(interaction_type);
            CREATE INDEX IF NOT EXISTS idx_ic_source ON interaction_classifications(source_type, source_id);

            -- Composite indexes for date-range and user-scoped queries (performance optimization)
            CREATE INDEX IF NOT EXISTS idx_ic_org_date ON interaction_classifications(organization_id, original_created_at);
            CREATE INDEX IF NOT EXISTS idx_ic_org_sender_date ON interaction_classifications(organization_id, sender_id, original_created_at);

            -- Workflow timelines table
            CREATE TABLE IF NOT EXISTS workflow_timelines (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,

                total_interactions INTEGER,
                total_participants INTEGER,
                total_duration_hours REAL,

                phases TEXT NOT NULL,
                key_events TEXT NOT NULL,
                bottlenecks TEXT,

                status TEXT NOT NULL,
                opened_at TEXT NOT NULL,
                closed_at TEXT,
                last_analyzed_at TEXT NOT NULL,

                UNIQUE(entity_type, entity_id)
            );

            CREATE INDEX IF NOT EXISTS idx_wt_org ON workflow_timelines(organization_id);
            CREATE INDEX IF NOT EXISTS idx_wt_entity ON workflow_timelines(entity_type, entity_id);

            -- Workflow patterns table
            CREATE TABLE IF NOT EXISTS workflow_patterns (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                pattern_type TEXT NOT NULL,
                pattern_name TEXT NOT NULL,
                description TEXT NOT NULL,
                criteria TEXT NOT NULL,

                occurrence_count INTEGER,
                success_correlation REAL,
                avg_time_impact_hours REAL,
                confidence_score REAL NOT NULL,

                examples TEXT NOT NULL,
                is_active INTEGER DEFAULT 1,

                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,

                UNIQUE(organization_id, pattern_name)
            );

            CREATE INDEX IF NOT EXISTS idx_wp_org ON workflow_patterns(organization_id);
            CREATE INDEX IF NOT EXISTS idx_wp_type ON workflow_patterns(pattern_type);

            -- Efficiency recommendations table
            CREATE TABLE IF NOT EXISTS efficiency_recommendations (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT,

                recommendation_type TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                suggested_actions TEXT NOT NULL,

                based_on_patterns TEXT NOT NULL,
                evidence TEXT NOT NULL,

                priority TEXT NOT NULL,
                estimated_time_savings_hours REAL,

                status TEXT DEFAULT 'pending',
                user_feedback TEXT,
                generated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_er_org ON efficiency_recommendations(organization_id);
            CREATE INDEX IF NOT EXISTS idx_er_target ON efficiency_recommendations(target_type, target_id);
            CREATE INDEX IF NOT EXISTS idx_er_status ON efficiency_recommendations(status);

            -- Analysis jobs table
            CREATE TABLE IF NOT EXISTS analysis_jobs (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,

                status TEXT NOT NULL,
                progress_percent INTEGER DEFAULT 0,
                current_stage TEXT,

                interactions_found INTEGER DEFAULT 0,
                interactions_classified INTEGER DEFAULT 0,
                patterns_matched INTEGER DEFAULT 0,
                recommendations_generated INTEGER DEFAULT 0,

                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_aj_org ON analysis_jobs(organization_id);
            CREATE INDEX IF NOT EXISTS idx_aj_entity ON analysis_jobs(entity_type, entity_id);
            CREATE INDEX IF NOT EXISTS idx_aj_status ON analysis_jobs(status);

            -- ==================== Phase 6: Team & Organization Aggregations ====================

            -- Team memberships (team = manager + direct reports)
            CREATE TABLE IF NOT EXISTS team_memberships (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                team_id TEXT NOT NULL,          -- manager's user_id
                team_name TEXT NOT NULL,        -- manager's name
                user_id TEXT NOT NULL,
                role TEXT DEFAULT 'member',     -- 'manager' or 'member'
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(organization_id, team_id, user_id)
            );

            CREATE INDEX IF NOT EXISTS idx_tm_org_team ON team_memberships(organization_id, team_id);
            CREATE INDEX IF NOT EXISTS idx_tm_org_user ON team_memberships(organization_id, user_id);

            -- Interaction type aggregations (time series)
            CREATE TABLE IF NOT EXISTS interaction_type_aggregations (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                team_id TEXT,                   -- NULL for org-level
                period_start TEXT NOT NULL,
                period_end TEXT NOT NULL,
                period_type TEXT NOT NULL,      -- 'daily', 'weekly', 'monthly'
                type_counts TEXT NOT NULL,      -- JSON: {"blocker": 5, ...}
                total_interactions INTEGER NOT NULL,
                clarification_ratio REAL,
                blocker_ratio REAL,
                escalation_ratio REAL,
                computed_at TEXT NOT NULL,
                UNIQUE(organization_id, team_id, period_start, period_type)
            );

            CREATE INDEX IF NOT EXISTS idx_ita_org_period ON interaction_type_aggregations(organization_id, period_start);
            CREATE INDEX IF NOT EXISTS idx_ita_team_period ON interaction_type_aggregations(organization_id, team_id, period_start);

            -- Sentiment aggregations
            CREATE TABLE IF NOT EXISTS sentiment_aggregations (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                team_id TEXT,
                period_start TEXT NOT NULL,
                period_end TEXT NOT NULL,
                period_type TEXT NOT NULL,
                avg_sentiment REAL NOT NULL,
                min_sentiment REAL,
                max_sentiment REAL,
                sentiment_std_dev REAL,
                positive_count INTEGER,
                neutral_count INTEGER,
                negative_count INTEGER,
                sentiment_by_type TEXT,         -- JSON
                rolling_7day_avg REAL,
                rolling_30day_avg REAL,
                total_interactions INTEGER NOT NULL,
                computed_at TEXT NOT NULL,
                UNIQUE(organization_id, team_id, period_start, period_type)
            );

            CREATE INDEX IF NOT EXISTS idx_sa_org_period ON sentiment_aggregations(organization_id, period_start);
            CREATE INDEX IF NOT EXISTS idx_sa_team_period ON sentiment_aggregations(organization_id, team_id, period_start);

            -- Bottleneck aggregations
            CREATE TABLE IF NOT EXISTS bottleneck_aggregations (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                team_id TEXT,
                period_start TEXT NOT NULL,
                period_end TEXT NOT NULL,
                period_type TEXT NOT NULL,
                type_counts TEXT NOT NULL,      -- JSON
                type_total_hours TEXT NOT NULL, -- JSON
                type_avg_hours TEXT NOT NULL,   -- JSON
                total_bottlenecks INTEGER NOT NULL,
                total_hours_lost REAL,
                avg_bottleneck_duration REAL,
                trend_direction TEXT,           -- 'improving', 'worsening', 'stable'
                trend_percent_change REAL,
                computed_at TEXT NOT NULL,
                UNIQUE(organization_id, team_id, period_start, period_type)
            );

            CREATE INDEX IF NOT EXISTS idx_ba_org_period ON bottleneck_aggregations(organization_id, period_start);
            CREATE INDEX IF NOT EXISTS idx_ba_team_period ON bottleneck_aggregations(organization_id, team_id, period_start);

            -- Participation network edges
            CREATE TABLE IF NOT EXISTS participation_edges (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                team_id TEXT,                   -- NULL for cross-team
                from_user_id TEXT NOT NULL,
                to_user_id TEXT NOT NULL,
                interaction_count INTEGER NOT NULL,
                avg_sentiment REAL,
                type_breakdown TEXT,            -- JSON
                period_start TEXT NOT NULL,
                period_end TEXT NOT NULL,
                weight REAL,
                computed_at TEXT NOT NULL,
                UNIQUE(organization_id, from_user_id, to_user_id, period_start)
            );

            CREATE INDEX IF NOT EXISTS idx_pe_org ON participation_edges(organization_id);
            CREATE INDEX IF NOT EXISTS idx_pe_team ON participation_edges(organization_id, team_id);
            CREATE INDEX IF NOT EXISTS idx_pe_from ON participation_edges(organization_id, from_user_id);
            CREATE INDEX IF NOT EXISTS idx_pe_to ON participation_edges(organization_id, to_user_id);

            -- Participation metrics (per user, per period)
            CREATE TABLE IF NOT EXISTS participation_metrics (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                team_id TEXT,
                user_id TEXT NOT NULL,
                period_start TEXT NOT NULL,
                period_end TEXT NOT NULL,
                period_type TEXT NOT NULL,
                degree_centrality REAL,
                betweenness_centrality REAL,
                closeness_centrality REAL,
                total_interactions_sent INTEGER,
                total_interactions_received INTEGER,
                unique_collaborators INTEGER,
                is_connector INTEGER,           -- Boolean
                is_bottleneck INTEGER,          -- Boolean
                computed_at TEXT NOT NULL,
                UNIQUE(organization_id, user_id, period_start, period_type)
            );

            CREATE INDEX IF NOT EXISTS idx_pm_org_user ON participation_metrics(organization_id, user_id);
            CREATE INDEX IF NOT EXISTS idx_pm_team ON participation_metrics(organization_id, team_id);

            -- Intervention outcomes (for learning)
            CREATE TABLE IF NOT EXISTS intervention_outcomes (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                recommendation_id TEXT NOT NULL,
                intervention_type TEXT NOT NULL,
                intervention_date TEXT NOT NULL,
                outcome_measured_date TEXT,
                outcome_type TEXT,
                outcome_value REAL,
                pre_intervention_metrics TEXT,
                post_intervention_metrics TEXT,
                confidence_score REAL,
                learned_pattern_id TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_io_org ON intervention_outcomes(organization_id);
            CREATE INDEX IF NOT EXISTS idx_io_rec ON intervention_outcomes(recommendation_id);
            CREATE INDEX IF NOT EXISTS idx_io_type ON intervention_outcomes(intervention_type);

            -- Aggregation jobs (batch processing)
            CREATE TABLE IF NOT EXISTS aggregation_jobs (
                id TEXT PRIMARY KEY,
                organization_id TEXT NOT NULL,
                job_type TEXT NOT NULL,
                scope TEXT NOT NULL,
                team_id TEXT,
                period_start TEXT NOT NULL,
                period_end TEXT NOT NULL,
                period_type TEXT NOT NULL,
                status TEXT NOT NULL,
                progress_percent INTEGER DEFAULT 0,
                records_processed INTEGER DEFAULT 0,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_agj_org_status ON aggregation_jobs(organization_id, status);
        "#)
        .map_err(|e| Error::Internal(format!("Failed to run analytics migrations: {}", e)))?;

        tracing::info!("Analytics database migrations complete");
        Ok(())
    }

    // ==================== Interaction Classifications ====================

    /// Delete all classifications for a task (used before re-analysis)
    pub fn delete_classifications_for_task(&self, task_id: &str) -> Result<u64> {
        let conn = self.conn()?;
        let count = conn.execute(
            "DELETE FROM interaction_classifications WHERE task_id = ?1",
            params![task_id],
        ).map_err(|e| Error::Internal(format!("Failed to delete classifications: {}", e)))?;
        Ok(count as u64)
    }

    /// Insert a classification
    pub fn insert_classification(&self, classification: &InteractionClassification) -> Result<()> {
        let conn = self.conn()?;

        let secondary_types_json = serde_json::to_string(&classification.secondary_types)
            .unwrap_or_else(|_| "[]".to_string());
        let entities_json = serde_json::to_string(&classification.entities)
            .unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            r#"
            INSERT OR REPLACE INTO interaction_classifications (
                id, organization_id, source_type, source_id, task_id, goal_id,
                sender_id, content, interaction_type, secondary_types, confidence_score,
                entities, sentiment, urgency_level, references_interaction_id,
                original_created_at, classified_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                classification.id.to_string(),
                classification.organization_id,
                classification.source_type.as_str(),
                classification.source_id,
                classification.task_id,
                classification.goal_id,
                classification.sender_id,
                classification.content,
                classification.interaction_type.as_str(),
                secondary_types_json,
                classification.confidence_score,
                entities_json,
                classification.sentiment,
                classification.urgency_level.as_str(),
                classification.references_interaction_id,
                classification.original_created_at.to_rfc3339(),
                classification.classified_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to insert classification: {}", e)))?;

        Ok(())
    }

    /// Get classifications for a task
    pub fn get_classifications_for_task(&self, task_id: &str) -> Result<Vec<InteractionClassification>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM interaction_classifications WHERE task_id = ?1 ORDER BY original_created_at ASC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![task_id], row_to_classification)
            .map_err(|e| Error::Internal(format!("Failed to query classifications: {}", e)))?
            .filter_map(|r| match r {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Failed to parse classification row: {}", e);
                    None
                }
            })
            .collect();

        Ok(records)
    }

    /// Get classifications for a goal
    pub fn get_classifications_for_goal(&self, goal_id: &str) -> Result<Vec<InteractionClassification>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM interaction_classifications WHERE goal_id = ?1 ORDER BY original_created_at ASC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![goal_id], row_to_classification)
            .map_err(|e| Error::Internal(format!("Failed to query classifications: {}", e)))?
            .filter_map(|r| match r {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Failed to parse classification row: {}", e);
                    None
                }
            })
            .collect();

        Ok(records)
    }

    /// Search classifications by type
    pub fn search_classifications_by_type(
        &self,
        organization_id: &str,
        interaction_type: InteractionType,
        limit: usize,
    ) -> Result<Vec<InteractionClassification>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM interaction_classifications WHERE organization_id = ?1 AND interaction_type = ?2 ORDER BY classified_at DESC LIMIT ?3"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(
            params![organization_id, interaction_type.as_str(), limit as i64],
            row_to_classification,
        )
        .map_err(|e| Error::Internal(format!("Failed to query classifications: {}", e)))?
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("Failed to parse classification row: {}", e);
                None
            }
        })
        .collect();

        Ok(records)
    }

    /// Check if a source has already been classified
    pub fn is_source_classified(&self, source_type: &str, source_id: &str) -> Result<bool> {
        let conn = self.conn()?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM interaction_classifications WHERE source_type = ?1 AND source_id = ?2",
            params![source_type, source_id],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to check classification: {}", e)))?;

        Ok(count > 0)
    }

    // ==================== Workflow Timelines ====================

    /// Upsert a workflow timeline
    pub fn upsert_timeline(&self, timeline: &WorkflowTimeline) -> Result<()> {
        let conn = self.conn()?;

        let phases_json = serde_json::to_string(&timeline.phases)
            .unwrap_or_else(|_| "[]".to_string());
        let events_json = serde_json::to_string(&timeline.key_events)
            .unwrap_or_else(|_| "[]".to_string());
        let bottlenecks_json = serde_json::to_string(&timeline.bottlenecks)
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            r#"
            INSERT INTO workflow_timelines (
                id, organization_id, entity_type, entity_id,
                total_interactions, total_participants, total_duration_hours,
                phases, key_events, bottlenecks,
                status, opened_at, closed_at, last_analyzed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                total_interactions = excluded.total_interactions,
                total_participants = excluded.total_participants,
                total_duration_hours = excluded.total_duration_hours,
                phases = excluded.phases,
                key_events = excluded.key_events,
                bottlenecks = excluded.bottlenecks,
                status = excluded.status,
                closed_at = excluded.closed_at,
                last_analyzed_at = excluded.last_analyzed_at
            "#,
            params![
                timeline.id.to_string(),
                timeline.organization_id,
                timeline.entity_type,
                timeline.entity_id,
                timeline.total_interactions,
                timeline.total_participants,
                timeline.total_duration_hours,
                phases_json,
                events_json,
                bottlenecks_json,
                timeline.status,
                timeline.opened_at.to_rfc3339(),
                timeline.closed_at.map(|t| t.to_rfc3339()),
                timeline.last_analyzed_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert timeline: {}", e)))?;

        Ok(())
    }

    /// Get timeline for an entity
    pub fn get_timeline(&self, entity_type: &str, entity_id: &str) -> Result<Option<WorkflowTimeline>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM workflow_timelines WHERE entity_type = ?1 AND entity_id = ?2"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![entity_type, entity_id], row_to_timeline)
            .optional()
            .map_err(|e| Error::Internal(format!("Failed to query timeline: {}", e)))?;

        Ok(record)
    }

    // ==================== Workflow Patterns ====================

    /// Upsert a pattern
    pub fn upsert_pattern(&self, pattern: &WorkflowPattern) -> Result<()> {
        let conn = self.conn()?;

        let criteria_json = serde_json::to_string(&pattern.criteria)
            .unwrap_or_else(|_| "{}".to_string());
        let examples_json = serde_json::to_string(&pattern.examples)
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            r#"
            INSERT INTO workflow_patterns (
                id, organization_id, pattern_type, pattern_name, description, criteria,
                occurrence_count, success_correlation, avg_time_impact_hours, confidence_score,
                examples, is_active, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(organization_id, pattern_name) DO UPDATE SET
                description = excluded.description,
                criteria = excluded.criteria,
                occurrence_count = excluded.occurrence_count,
                success_correlation = excluded.success_correlation,
                avg_time_impact_hours = excluded.avg_time_impact_hours,
                confidence_score = excluded.confidence_score,
                examples = excluded.examples,
                updated_at = excluded.updated_at
            "#,
            params![
                pattern.id.to_string(),
                pattern.organization_id,
                pattern.pattern_type.as_str(),
                pattern.pattern_name,
                pattern.description,
                criteria_json,
                pattern.occurrence_count,
                pattern.success_correlation,
                pattern.avg_time_impact_hours,
                pattern.confidence_score,
                examples_json,
                pattern.is_active as i32,
                pattern.created_at.to_rfc3339(),
                pattern.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert pattern: {}", e)))?;

        Ok(())
    }

    /// Get all active patterns for an organization
    pub fn get_patterns(&self, organization_id: &str) -> Result<Vec<WorkflowPattern>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM workflow_patterns WHERE organization_id = ?1 AND is_active = 1 ORDER BY confidence_score DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id], row_to_pattern)
            .map_err(|e| Error::Internal(format!("Failed to query patterns: {}", e)))?
            .filter_map(|r| match r {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Failed to parse pattern row: {}", e);
                    None
                }
            })
            .collect();

        Ok(records)
    }

    // ==================== Recommendations ====================

    /// Insert a recommendation
    pub fn insert_recommendation(&self, rec: &EfficiencyRecommendation) -> Result<()> {
        let conn = self.conn()?;

        let actions_json = serde_json::to_string(&rec.suggested_actions)
            .unwrap_or_else(|_| "[]".to_string());
        let patterns_json = serde_json::to_string(&rec.based_on_patterns)
            .unwrap_or_else(|_| "[]".to_string());
        let evidence_json = serde_json::to_string(&rec.evidence)
            .unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            r#"
            INSERT INTO efficiency_recommendations (
                id, organization_id, target_type, target_id,
                recommendation_type, title, description, suggested_actions,
                based_on_patterns, evidence, priority, estimated_time_savings_hours,
                status, user_feedback, generated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                rec.id.to_string(),
                rec.organization_id,
                rec.target_type.as_str(),
                rec.target_id,
                rec.recommendation_type.as_str(),
                rec.title,
                rec.description,
                actions_json,
                patterns_json,
                evidence_json,
                rec.priority.as_str(),
                rec.estimated_time_savings_hours,
                rec.status.as_str(),
                rec.user_feedback,
                rec.generated_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to insert recommendation: {}", e)))?;

        Ok(())
    }

    /// Get recommendations for a target
    pub fn get_recommendations_for_target(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Vec<EfficiencyRecommendation>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM efficiency_recommendations WHERE target_type = ?1 AND target_id = ?2 ORDER BY generated_at DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![target_type, target_id], row_to_recommendation)
            .map_err(|e| Error::Internal(format!("Failed to query recommendations: {}", e)))?
            .filter_map(|r| match r {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Failed to parse recommendation row: {}", e);
                    None
                }
            })
            .collect();

        Ok(records)
    }

    /// Get org-wide recommendations
    pub fn get_org_recommendations(&self, organization_id: &str, limit: usize) -> Result<Vec<EfficiencyRecommendation>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM efficiency_recommendations WHERE organization_id = ?1 AND status = 'pending' ORDER BY priority DESC, generated_at DESC LIMIT ?2"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id, limit as i64], row_to_recommendation)
            .map_err(|e| Error::Internal(format!("Failed to query recommendations: {}", e)))?
            .filter_map(|r| match r {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Failed to parse recommendation row: {}", e);
                    None
                }
            })
            .collect();

        Ok(records)
    }

    /// Update recommendation status with feedback
    pub fn update_recommendation_feedback(
        &self,
        id: &Uuid,
        status: RecommendationStatus,
        feedback: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn()?;

        let count = conn.execute(
            "UPDATE efficiency_recommendations SET status = ?2, user_feedback = ?3 WHERE id = ?1",
            params![id.to_string(), status.as_str(), feedback],
        ).map_err(|e| Error::Internal(format!("Failed to update recommendation: {}", e)))?;

        Ok(count > 0)
    }

    /// Get all recommendations for an organization (any status)
    pub fn get_recommendations_for_organization(&self, organization_id: &str, limit: usize) -> Result<Vec<EfficiencyRecommendation>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM efficiency_recommendations WHERE organization_id = ?1 ORDER BY generated_at DESC LIMIT ?2"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id, limit as i64], row_to_recommendation)
            .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Get recommendations based on a specific pattern
    pub fn get_recommendations_by_pattern(&self, organization_id: &str, pattern_id: &Uuid) -> Result<Vec<EfficiencyRecommendation>> {
        let conn = self.conn()?;
        let pattern_json_match = format!("%{}%", pattern_id);

        let mut stmt = conn.prepare(
            "SELECT * FROM efficiency_recommendations WHERE organization_id = ?1 AND based_on_patterns LIKE ?2 ORDER BY generated_at DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id, pattern_json_match], row_to_recommendation)
            .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Get active patterns for an organization
    pub fn get_active_patterns(&self, organization_id: &str) -> Result<Vec<WorkflowPattern>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM workflow_patterns WHERE organization_id = ?1 AND is_active = 1 ORDER BY confidence_score DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id], row_to_pattern)
            .map_err(|e| Error::Internal(format!("Failed to query patterns: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Update pattern confidence score
    pub fn update_pattern_confidence(&self, pattern_id: &Uuid, new_confidence: f32) -> Result<bool> {
        let conn = self.conn()?;
        let count = conn.execute(
            "UPDATE workflow_patterns SET confidence_score = ?2, updated_at = ?3 WHERE id = ?1",
            params![pattern_id.to_string(), new_confidence, Utc::now().to_rfc3339()],
        ).map_err(|e| Error::Internal(format!("Failed to update pattern confidence: {}", e)))?;

        Ok(count > 0)
    }

    // ==================== Analysis Jobs ====================

    /// Create an analysis job
    pub fn create_analysis_job(&self, job: &AnalysisJob) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            r#"
            INSERT INTO analysis_jobs (
                id, organization_id, entity_type, entity_id,
                status, progress_percent, current_stage,
                interactions_found, interactions_classified, patterns_matched, recommendations_generated,
                error, created_at, updated_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                job.id.to_string(),
                job.organization_id,
                job.entity_type,
                job.entity_id,
                job.status.as_str(),
                job.progress_percent as i32,
                job.current_stage,
                job.interactions_found,
                job.interactions_classified,
                job.patterns_matched,
                job.recommendations_generated,
                job.error,
                job.created_at.to_rfc3339(),
                job.updated_at.to_rfc3339(),
                job.completed_at.map(|t| t.to_rfc3339()),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to create analysis job: {}", e)))?;

        Ok(())
    }

    /// Update analysis job progress
    pub fn update_analysis_job(&self, job: &AnalysisJob) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            r#"
            UPDATE analysis_jobs SET
                status = ?2,
                progress_percent = ?3,
                current_stage = ?4,
                interactions_found = ?5,
                interactions_classified = ?6,
                patterns_matched = ?7,
                recommendations_generated = ?8,
                error = ?9,
                updated_at = ?10,
                completed_at = ?11
            WHERE id = ?1
            "#,
            params![
                job.id.to_string(),
                job.status.as_str(),
                job.progress_percent as i32,
                job.current_stage,
                job.interactions_found,
                job.interactions_classified,
                job.patterns_matched,
                job.recommendations_generated,
                job.error,
                job.updated_at.to_rfc3339(),
                job.completed_at.map(|t| t.to_rfc3339()),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to update analysis job: {}", e)))?;

        Ok(())
    }

    /// Get analysis job by ID
    pub fn get_analysis_job(&self, id: &Uuid) -> Result<Option<AnalysisJob>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM analysis_jobs WHERE id = ?1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![id.to_string()], row_to_analysis_job)
            .optional()
            .map_err(|e| Error::Internal(format!("Failed to query analysis job: {}", e)))?;

        Ok(record)
    }

    // ==================== Team Memberships ====================

    /// Clear all team memberships for an organization (for sync)
    pub fn clear_team_memberships(&self, organization_id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM team_memberships WHERE organization_id = ?1",
            params![organization_id],
        ).map_err(|e| Error::Internal(format!("Failed to clear team memberships: {}", e)))?;
        Ok(())
    }

    /// Insert a team membership
    pub fn insert_team_membership(&self, membership: &super::aggregation_types::TeamMembership) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            r#"
            INSERT INTO team_memberships (
                id, organization_id, team_id, team_name, user_id, role, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(organization_id, team_id, user_id) DO UPDATE SET
                team_name = excluded.team_name,
                role = excluded.role,
                updated_at = excluded.updated_at
            "#,
            params![
                membership.id.to_string(),
                membership.organization_id,
                membership.team_id,
                membership.team_name,
                membership.user_id,
                membership.role.as_str(),
                membership.created_at.to_rfc3339(),
                membership.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to insert team membership: {}", e)))?;
        Ok(())
    }

    /// List all teams in an organization
    pub fn list_teams(&self, organization_id: &str) -> Result<Vec<super::team_manager::TeamInfo>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT team_id, team_name, COUNT(*) as member_count
            FROM team_memberships
            WHERE organization_id = ?1
            GROUP BY team_id, team_name
            ORDER BY team_name
            "#
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let teams = stmt.query_map(params![organization_id], |row| {
            Ok(super::team_manager::TeamInfo {
                team_id: row.get(0)?,
                team_name: row.get(1)?,
                member_count: row.get::<_, i32>(2)? as u32,
            })
        })
        .map_err(|e| Error::Internal(format!("Failed to query teams: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(teams)
    }

    /// Get members of a team
    pub fn get_team_members(
        &self,
        organization_id: &str,
        team_id: &str,
    ) -> Result<Vec<super::aggregation_types::TeamMembership>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM team_memberships WHERE organization_id = ?1 AND team_id = ?2 ORDER BY role DESC, user_id"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let members = stmt.query_map(params![organization_id, team_id], row_to_team_membership)
            .map_err(|e| Error::Internal(format!("Failed to query team members: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(members)
    }

    /// Get the team a user belongs to
    pub fn get_user_team(
        &self,
        organization_id: &str,
        user_id: &str,
    ) -> Result<Option<super::aggregation_types::TeamMembership>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM team_memberships WHERE organization_id = ?1 AND user_id = ?2 LIMIT 1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let membership = stmt.query_row(params![organization_id, user_id], row_to_team_membership)
            .optional()
            .map_err(|e| Error::Internal(format!("Failed to query user team: {}", e)))?;

        Ok(membership)
    }

    /// Remove a team member
    pub fn remove_team_member(
        &self,
        organization_id: &str,
        team_id: &str,
        user_id: &str,
    ) -> Result<bool> {
        let conn = self.conn()?;
        let count = conn.execute(
            "DELETE FROM team_memberships WHERE organization_id = ?1 AND team_id = ?2 AND user_id = ?3",
            params![organization_id, team_id, user_id],
        ).map_err(|e| Error::Internal(format!("Failed to remove team member: {}", e)))?;
        Ok(count > 0)
    }

    // ==================== Interaction Type Aggregations ====================

    /// Upsert an interaction type aggregation
    pub fn upsert_interaction_type_aggregation(
        &self,
        agg: &super::aggregation_types::InteractionTypeAggregation,
    ) -> Result<()> {
        let conn = self.conn()?;
        let type_counts_json = serde_json::to_string(&agg.type_counts).unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            r#"
            INSERT INTO interaction_type_aggregations (
                id, organization_id, team_id, period_start, period_end, period_type,
                type_counts, total_interactions, clarification_ratio, blocker_ratio, escalation_ratio, computed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(organization_id, team_id, period_start, period_type) DO UPDATE SET
                type_counts = excluded.type_counts,
                total_interactions = excluded.total_interactions,
                clarification_ratio = excluded.clarification_ratio,
                blocker_ratio = excluded.blocker_ratio,
                escalation_ratio = excluded.escalation_ratio,
                computed_at = excluded.computed_at
            "#,
            params![
                agg.id.to_string(),
                agg.organization_id,
                agg.team_id,
                agg.period_start.to_rfc3339(),
                agg.period_end.to_rfc3339(),
                agg.period_type.as_str(),
                type_counts_json,
                agg.total_interactions,
                agg.clarification_ratio,
                agg.blocker_ratio,
                agg.escalation_ratio,
                agg.computed_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert interaction type aggregation: {}", e)))?;
        Ok(())
    }

    /// Get interaction type aggregations for a period
    pub fn get_interaction_type_aggregations(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_type: &str,
        limit: usize,
    ) -> Result<Vec<super::aggregation_types::InteractionTypeAggregation>> {
        let conn = self.conn()?;

        let query = if team_id.is_some() {
            "SELECT * FROM interaction_type_aggregations WHERE organization_id = ?1 AND team_id = ?2 AND period_type = ?3 ORDER BY period_start DESC LIMIT ?4"
        } else {
            "SELECT * FROM interaction_type_aggregations WHERE organization_id = ?1 AND team_id IS NULL AND period_type = ?2 ORDER BY period_start DESC LIMIT ?3"
        };

        let mut stmt = conn.prepare(query)
            .map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let results: Vec<super::aggregation_types::InteractionTypeAggregation> = if let Some(tid) = team_id {
            stmt.query_map(params![organization_id, tid, period_type, limit as i64], row_to_interaction_type_agg)
                .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
                .filter_map(|r| r.ok())
                .collect()
        } else {
            stmt.query_map(params![organization_id, period_type, limit as i64], row_to_interaction_type_agg)
                .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
                .filter_map(|r| r.ok())
                .collect()
        };

        Ok(results)
    }

    // ==================== Sentiment Aggregations ====================

    /// Upsert a sentiment aggregation
    pub fn upsert_sentiment_aggregation(
        &self,
        agg: &super::aggregation_types::SentimentAggregation,
    ) -> Result<()> {
        let conn = self.conn()?;
        let by_type_json = serde_json::to_string(&agg.sentiment_by_type).unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            r#"
            INSERT INTO sentiment_aggregations (
                id, organization_id, team_id, period_start, period_end, period_type,
                avg_sentiment, min_sentiment, max_sentiment, sentiment_std_dev,
                positive_count, neutral_count, negative_count, sentiment_by_type,
                rolling_7day_avg, rolling_30day_avg, total_interactions, computed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(organization_id, team_id, period_start, period_type) DO UPDATE SET
                avg_sentiment = excluded.avg_sentiment,
                min_sentiment = excluded.min_sentiment,
                max_sentiment = excluded.max_sentiment,
                sentiment_std_dev = excluded.sentiment_std_dev,
                positive_count = excluded.positive_count,
                neutral_count = excluded.neutral_count,
                negative_count = excluded.negative_count,
                sentiment_by_type = excluded.sentiment_by_type,
                rolling_7day_avg = excluded.rolling_7day_avg,
                rolling_30day_avg = excluded.rolling_30day_avg,
                total_interactions = excluded.total_interactions,
                computed_at = excluded.computed_at
            "#,
            params![
                agg.id.to_string(),
                agg.organization_id,
                agg.team_id,
                agg.period_start.to_rfc3339(),
                agg.period_end.to_rfc3339(),
                agg.period_type.as_str(),
                agg.avg_sentiment,
                agg.min_sentiment,
                agg.max_sentiment,
                agg.sentiment_std_dev,
                agg.positive_count,
                agg.neutral_count,
                agg.negative_count,
                by_type_json,
                agg.rolling_7day_avg,
                agg.rolling_30day_avg,
                agg.total_interactions,
                agg.computed_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert sentiment aggregation: {}", e)))?;
        Ok(())
    }

    // ==================== Bottleneck Aggregations ====================

    /// Get bottleneck aggregation for a specific period
    pub fn get_bottleneck_aggregation(
        &self,
        organization_id: &str,
        team_id: Option<&str>,
        period_start: &DateTime<Utc>,
        period_type: &str,
    ) -> Result<Option<super::aggregation_types::BottleneckAggregation>> {
        let conn = self.conn()?;

        let query = if team_id.is_some() {
            "SELECT * FROM bottleneck_aggregations WHERE organization_id = ?1 AND team_id = ?2 AND period_start = ?3 AND period_type = ?4 LIMIT 1"
        } else {
            "SELECT * FROM bottleneck_aggregations WHERE organization_id = ?1 AND team_id IS NULL AND period_start = ?2 AND period_type = ?3 LIMIT 1"
        };

        let mut stmt = conn.prepare(query)
            .map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let result: Option<super::aggregation_types::BottleneckAggregation> = if let Some(tid) = team_id {
            stmt.query_row(params![organization_id, tid, period_start.to_rfc3339(), period_type], row_to_bottleneck_agg)
                .optional()
                .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
        } else {
            stmt.query_row(params![organization_id, period_start.to_rfc3339(), period_type], row_to_bottleneck_agg)
                .optional()
                .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
        };

        Ok(result)
    }

    /// Upsert a bottleneck aggregation
    pub fn upsert_bottleneck_aggregation(
        &self,
        agg: &super::aggregation_types::BottleneckAggregation,
    ) -> Result<()> {
        let conn = self.conn()?;
        let type_counts_json = serde_json::to_string(&agg.type_counts).unwrap_or_else(|_| "{}".to_string());
        let total_hours_json = serde_json::to_string(&agg.type_total_hours).unwrap_or_else(|_| "{}".to_string());
        let avg_hours_json = serde_json::to_string(&agg.type_avg_hours).unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            r#"
            INSERT INTO bottleneck_aggregations (
                id, organization_id, team_id, period_start, period_end, period_type,
                type_counts, type_total_hours, type_avg_hours, total_bottlenecks,
                total_hours_lost, avg_bottleneck_duration, trend_direction, trend_percent_change, computed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(organization_id, team_id, period_start, period_type) DO UPDATE SET
                type_counts = excluded.type_counts,
                type_total_hours = excluded.type_total_hours,
                type_avg_hours = excluded.type_avg_hours,
                total_bottlenecks = excluded.total_bottlenecks,
                total_hours_lost = excluded.total_hours_lost,
                avg_bottleneck_duration = excluded.avg_bottleneck_duration,
                trend_direction = excluded.trend_direction,
                trend_percent_change = excluded.trend_percent_change,
                computed_at = excluded.computed_at
            "#,
            params![
                agg.id.to_string(),
                agg.organization_id,
                agg.team_id,
                agg.period_start.to_rfc3339(),
                agg.period_end.to_rfc3339(),
                agg.period_type.as_str(),
                type_counts_json,
                total_hours_json,
                avg_hours_json,
                agg.total_bottlenecks,
                agg.total_hours_lost,
                agg.avg_bottleneck_duration,
                agg.trend_direction.as_str(),
                agg.trend_percent_change,
                agg.computed_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert bottleneck aggregation: {}", e)))?;
        Ok(())
    }

    // ==================== Participation Network ====================

    /// Upsert a participation edge
    pub fn upsert_participation_edge(
        &self,
        edge: &super::aggregation_types::ParticipationEdge,
    ) -> Result<()> {
        let conn = self.conn()?;
        let type_breakdown_json = serde_json::to_string(&edge.type_breakdown).unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            r#"
            INSERT INTO participation_edges (
                id, organization_id, team_id, from_user_id, to_user_id,
                interaction_count, avg_sentiment, type_breakdown, period_start, period_end, weight, computed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(organization_id, from_user_id, to_user_id, period_start) DO UPDATE SET
                team_id = excluded.team_id,
                interaction_count = excluded.interaction_count,
                avg_sentiment = excluded.avg_sentiment,
                type_breakdown = excluded.type_breakdown,
                weight = excluded.weight,
                computed_at = excluded.computed_at
            "#,
            params![
                edge.id.to_string(),
                edge.organization_id,
                edge.team_id,
                edge.from_user_id,
                edge.to_user_id,
                edge.interaction_count,
                edge.avg_sentiment,
                type_breakdown_json,
                edge.period_start.to_rfc3339(),
                edge.period_end.to_rfc3339(),
                edge.weight,
                edge.computed_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert participation edge: {}", e)))?;
        Ok(())
    }

    /// Upsert participation metrics for a user
    pub fn upsert_participation_metrics(
        &self,
        metrics: &super::aggregation_types::ParticipationMetrics,
    ) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            r#"
            INSERT INTO participation_metrics (
                id, organization_id, team_id, user_id, period_start, period_end, period_type,
                degree_centrality, betweenness_centrality, closeness_centrality,
                total_interactions_sent, total_interactions_received, unique_collaborators,
                is_connector, is_bottleneck, computed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(organization_id, user_id, period_start, period_type) DO UPDATE SET
                team_id = excluded.team_id,
                degree_centrality = excluded.degree_centrality,
                betweenness_centrality = excluded.betweenness_centrality,
                closeness_centrality = excluded.closeness_centrality,
                total_interactions_sent = excluded.total_interactions_sent,
                total_interactions_received = excluded.total_interactions_received,
                unique_collaborators = excluded.unique_collaborators,
                is_connector = excluded.is_connector,
                is_bottleneck = excluded.is_bottleneck,
                computed_at = excluded.computed_at
            "#,
            params![
                metrics.id.to_string(),
                metrics.organization_id,
                metrics.team_id,
                metrics.user_id,
                metrics.period_start.to_rfc3339(),
                metrics.period_end.to_rfc3339(),
                metrics.period_type.as_str(),
                metrics.degree_centrality,
                metrics.betweenness_centrality,
                metrics.closeness_centrality,
                metrics.total_interactions_sent,
                metrics.total_interactions_received,
                metrics.unique_collaborators,
                metrics.is_connector as i32,
                metrics.is_bottleneck as i32,
                metrics.computed_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert participation metrics: {}", e)))?;
        Ok(())
    }

    // ==================== Intervention Outcomes ====================

    /// Insert an intervention outcome
    pub fn insert_intervention_outcome(
        &self,
        outcome: &super::aggregation_types::InterventionOutcome,
    ) -> Result<()> {
        let conn = self.conn()?;
        let pre_metrics_json = outcome.pre_intervention_metrics.as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
        let post_metrics_json = outcome.post_intervention_metrics.as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));

        conn.execute(
            r#"
            INSERT INTO intervention_outcomes (
                id, organization_id, recommendation_id, intervention_type, intervention_date,
                outcome_measured_date, outcome_type, outcome_value,
                pre_intervention_metrics, post_intervention_metrics,
                confidence_score, learned_pattern_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                outcome.id.to_string(),
                outcome.organization_id,
                outcome.recommendation_id.to_string(),
                outcome.intervention_type,
                outcome.intervention_date.to_rfc3339(),
                outcome.outcome_measured_date.map(|d| d.to_rfc3339()),
                outcome.outcome_type,
                outcome.outcome_value,
                pre_metrics_json,
                post_metrics_json,
                outcome.confidence_score,
                outcome.learned_pattern_id.map(|id| id.to_string()),
                outcome.created_at.to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to insert intervention outcome: {}", e)))?;
        Ok(())
    }

    /// Update intervention outcome with measured results
    pub fn update_intervention_outcome(
        &self,
        id: &Uuid,
        outcome_type: &str,
        outcome_value: f64,
        post_metrics: &serde_json::Value,
    ) -> Result<bool> {
        let conn = self.conn()?;
        let post_json = serde_json::to_string(post_metrics).unwrap_or_else(|_| "{}".to_string());

        let count = conn.execute(
            r#"
            UPDATE intervention_outcomes SET
                outcome_measured_date = ?2,
                outcome_type = ?3,
                outcome_value = ?4,
                post_intervention_metrics = ?5
            WHERE id = ?1
            "#,
            params![
                id.to_string(),
                Utc::now().to_rfc3339(),
                outcome_type,
                outcome_value,
                post_json,
            ],
        ).map_err(|e| Error::Internal(format!("Failed to update intervention outcome: {}", e)))?;

        Ok(count > 0)
    }

    /// Get intervention outcomes for a recommendation
    pub fn get_intervention_outcomes(
        &self,
        recommendation_id: &Uuid,
    ) -> Result<Vec<super::aggregation_types::InterventionOutcome>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM intervention_outcomes WHERE recommendation_id = ?1 ORDER BY created_at DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let outcomes = stmt.query_map(params![recommendation_id.to_string()], row_to_intervention_outcome)
            .map_err(|e| Error::Internal(format!("Failed to query: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(outcomes)
    }

    /// Get all classifications for an organization within a date range
    pub fn get_classifications_in_range(
        &self,
        organization_id: &str,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> Result<Vec<InteractionClassification>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM interaction_classifications WHERE organization_id = ?1 AND original_created_at >= ?2 AND original_created_at < ?3 ORDER BY original_created_at ASC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(
            params![organization_id, start.to_rfc3339(), end.to_rfc3339()],
            row_to_classification,
        )
        .map_err(|e| Error::Internal(format!("Failed to query classifications: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(records)
    }

    /// Get classifications for a specific user within a date range
    /// Pushes sender_id filtering into SQL for efficiency (uses idx_ic_org_sender_date)
    pub fn get_classifications_for_user_in_range(
        &self,
        organization_id: &str,
        sender_id: &str,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> Result<Vec<InteractionClassification>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM interaction_classifications \
             WHERE organization_id = ?1 AND sender_id = ?2 \
             AND original_created_at >= ?3 AND original_created_at < ?4 \
             ORDER BY original_created_at ASC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(
            params![organization_id, sender_id, start.to_rfc3339(), end.to_rfc3339()],
            row_to_classification,
        )
        .map_err(|e| Error::Internal(format!("Failed to query classifications: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(records)
    }

    /// Get all timelines for an organization
    pub fn get_timelines_for_org(&self, organization_id: &str) -> Result<Vec<WorkflowTimeline>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM workflow_timelines WHERE organization_id = ?1 ORDER BY last_analyzed_at DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id], row_to_timeline)
            .map_err(|e| Error::Internal(format!("Failed to query timelines: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }
}

// ==================== Row Converters ====================

fn row_to_classification(row: &rusqlite::Row) -> rusqlite::Result<InteractionClassification> {
    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let source_type_str: String = row.get(2)?;
    let source_id: String = row.get(3)?;
    let task_id: Option<String> = row.get(4)?;
    let goal_id: Option<String> = row.get(5)?;
    let sender_id: String = row.get(6)?;
    let content: String = row.get(7)?;
    let interaction_type_str: String = row.get(8)?;
    let secondary_types_json: Option<String> = row.get(9)?;
    let confidence_score: f64 = row.get(10)?;
    let entities_json: Option<String> = row.get(11)?;
    let sentiment: Option<f64> = row.get(12)?;
    let urgency_str: Option<String> = row.get(13)?;
    let references_id: Option<String> = row.get(14)?;
    let original_at_str: String = row.get(15)?;
    let classified_at_str: String = row.get(16)?;

    Ok(InteractionClassification {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        source_type: InteractionSource::parse(&source_type_str),
        source_id,
        task_id,
        goal_id,
        sender_id,
        content,
        interaction_type: InteractionType::parse(&interaction_type_str),
        secondary_types: secondary_types_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        confidence_score: confidence_score as f32,
        entities: entities_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        sentiment: sentiment.unwrap_or(0.0) as f32,
        urgency_level: urgency_str.map(|s| UrgencyLevel::parse(&s)).unwrap_or(UrgencyLevel::Medium),
        references_interaction_id: references_id,
        original_created_at: DateTime::parse_from_rfc3339(&original_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        classified_at: DateTime::parse_from_rfc3339(&classified_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_timeline(row: &rusqlite::Row) -> rusqlite::Result<WorkflowTimeline> {
    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let entity_type: String = row.get(2)?;
    let entity_id: String = row.get(3)?;
    let total_interactions: Option<i32> = row.get(4)?;
    let total_participants: Option<i32> = row.get(5)?;
    let total_duration: Option<f64> = row.get(6)?;
    let phases_json: String = row.get(7)?;
    let events_json: String = row.get(8)?;
    let bottlenecks_json: Option<String> = row.get(9)?;
    let status: String = row.get(10)?;
    let opened_at_str: String = row.get(11)?;
    let closed_at_str: Option<String> = row.get(12)?;
    let analyzed_at_str: String = row.get(13)?;

    Ok(WorkflowTimeline {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        entity_type,
        entity_id,
        total_interactions: total_interactions.unwrap_or(0).max(0) as u32,
        total_participants: total_participants.unwrap_or(0).max(0) as u32,
        total_duration_hours: total_duration,
        phases: serde_json::from_str(&phases_json).unwrap_or_default(),
        key_events: serde_json::from_str(&events_json).unwrap_or_default(),
        bottlenecks: bottlenecks_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        status,
        opened_at: DateTime::parse_from_rfc3339(&opened_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        closed_at: closed_at_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)).ok()
        }),
        last_analyzed_at: DateTime::parse_from_rfc3339(&analyzed_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_pattern(row: &rusqlite::Row) -> rusqlite::Result<WorkflowPattern> {
    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let pattern_type_str: String = row.get(2)?;
    let pattern_name: String = row.get(3)?;
    let description: String = row.get(4)?;
    let criteria_json: String = row.get(5)?;
    let occurrence_count: Option<i32> = row.get(6)?;
    let success_correlation: Option<f64> = row.get(7)?;
    let avg_time_impact: Option<f64> = row.get(8)?;
    let confidence_score: f64 = row.get(9)?;
    let examples_json: String = row.get(10)?;
    let is_active: i32 = row.get(11)?;
    let created_at_str: String = row.get(12)?;
    let updated_at_str: String = row.get(13)?;

    Ok(WorkflowPattern {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        pattern_type: PatternType::parse(&pattern_type_str),
        pattern_name,
        description,
        criteria: serde_json::from_str(&criteria_json).unwrap_or(serde_json::json!({})),
        occurrence_count: occurrence_count.unwrap_or(0).max(0) as u32,
        success_correlation: success_correlation.map(|s| (s as f32).clamp(-1.0, 1.0)),
        avg_time_impact_hours: avg_time_impact,
        confidence_score: (confidence_score as f32).clamp(0.0, 1.0),
        examples: serde_json::from_str(&examples_json).unwrap_or_default(),
        is_active: is_active != 0,
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_recommendation(row: &rusqlite::Row) -> rusqlite::Result<EfficiencyRecommendation> {
    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let target_type_str: String = row.get(2)?;
    let target_id: Option<String> = row.get(3)?;
    let rec_type_str: String = row.get(4)?;
    let title: String = row.get(5)?;
    let description: String = row.get(6)?;
    let actions_json: String = row.get(7)?;
    let patterns_json: String = row.get(8)?;
    let evidence_json: String = row.get(9)?;
    let priority_str: String = row.get(10)?;
    let time_savings: Option<f64> = row.get(11)?;
    let status_str: String = row.get(12)?;
    let user_feedback: Option<String> = row.get(13)?;
    let generated_at_str: String = row.get(14)?;

    Ok(EfficiencyRecommendation {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        target_type: RecommendationTarget::parse(&target_type_str),
        target_id,
        recommendation_type: RecommendationType::parse(&rec_type_str),
        title,
        description,
        suggested_actions: serde_json::from_str(&actions_json).unwrap_or_default(),
        based_on_patterns: serde_json::from_str(&patterns_json).unwrap_or_default(),
        evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::json!({})),
        priority: UrgencyLevel::parse(&priority_str),
        estimated_time_savings_hours: time_savings,
        status: RecommendationStatus::parse(&status_str),
        user_feedback,
        generated_at: DateTime::parse_from_rfc3339(&generated_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_analysis_job(row: &rusqlite::Row) -> rusqlite::Result<AnalysisJob> {
    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let entity_type: String = row.get(2)?;
    let entity_id: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let progress: i32 = row.get(5)?;
    let current_stage: Option<String> = row.get(6)?;
    let interactions_found: i32 = row.get(7)?;
    let interactions_classified: i32 = row.get(8)?;
    let patterns_matched: i32 = row.get(9)?;
    let recommendations_generated: i32 = row.get(10)?;
    let error: Option<String> = row.get(11)?;
    let created_at_str: String = row.get(12)?;
    let updated_at_str: String = row.get(13)?;
    let completed_at_str: Option<String> = row.get(14)?;

    // Clamp progress to valid u8 range (0-100 for percentage)
    let progress_clamped = progress.clamp(0, 100) as u8;

    Ok(AnalysisJob {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        entity_type,
        entity_id,
        status: AnalysisJobStatus::parse(&status_str),
        progress_percent: progress_clamped,
        current_stage: current_stage.unwrap_or_else(|| "unknown".to_string()),
        interactions_found: interactions_found.max(0) as u32,
        interactions_classified: interactions_classified.max(0) as u32,
        patterns_matched: patterns_matched.max(0) as u32,
        recommendations_generated: recommendations_generated.max(0) as u32,
        error,
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        completed_at: completed_at_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)).ok()
        }),
    })
}

fn row_to_team_membership(row: &rusqlite::Row) -> rusqlite::Result<super::aggregation_types::TeamMembership> {
    use super::aggregation_types::{TeamMembership, TeamRole};

    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let team_id: String = row.get(2)?;
    let team_name: String = row.get(3)?;
    let user_id: String = row.get(4)?;
    let role_str: String = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let updated_at_str: String = row.get(7)?;

    Ok(TeamMembership {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        team_id,
        team_name,
        user_id,
        role: TeamRole::parse(&role_str),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_interaction_type_agg(row: &rusqlite::Row) -> rusqlite::Result<super::aggregation_types::InteractionTypeAggregation> {
    use super::aggregation_types::{InteractionTypeAggregation, PeriodType};

    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let team_id: Option<String> = row.get(2)?;
    let period_start_str: String = row.get(3)?;
    let period_end_str: String = row.get(4)?;
    let period_type_str: String = row.get(5)?;
    let type_counts_json: String = row.get(6)?;
    let total_interactions: i32 = row.get(7)?;
    let clarification_ratio: Option<f64> = row.get(8)?;
    let blocker_ratio: Option<f64> = row.get(9)?;
    let escalation_ratio: Option<f64> = row.get(10)?;
    let computed_at_str: String = row.get(11)?;

    Ok(InteractionTypeAggregation {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        team_id,
        period_start: DateTime::parse_from_rfc3339(&period_start_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        period_end: DateTime::parse_from_rfc3339(&period_end_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        period_type: PeriodType::parse(&period_type_str),
        type_counts: serde_json::from_str(&type_counts_json).unwrap_or_default(),
        total_interactions: total_interactions.max(0) as u32,
        clarification_ratio: clarification_ratio.unwrap_or(0.0) as f32,
        blocker_ratio: blocker_ratio.unwrap_or(0.0) as f32,
        escalation_ratio: escalation_ratio.unwrap_or(0.0) as f32,
        computed_at: DateTime::parse_from_rfc3339(&computed_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_bottleneck_agg(row: &rusqlite::Row) -> rusqlite::Result<super::aggregation_types::BottleneckAggregation> {
    use super::aggregation_types::{BottleneckAggregation, PeriodType, TrendDirection};

    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let team_id: Option<String> = row.get(2)?;
    let period_start_str: String = row.get(3)?;
    let period_end_str: String = row.get(4)?;
    let period_type_str: String = row.get(5)?;
    let type_counts_json: String = row.get(6)?;
    let type_total_hours_json: String = row.get(7)?;
    let type_avg_hours_json: String = row.get(8)?;
    let total_bottlenecks: i32 = row.get(9)?;
    let total_hours_lost: Option<f64> = row.get(10)?;
    let avg_bottleneck_duration: Option<f64> = row.get(11)?;
    let trend_direction_str: Option<String> = row.get(12)?;
    let trend_percent_change: Option<f64> = row.get(13)?;
    let computed_at_str: String = row.get(14)?;

    Ok(BottleneckAggregation {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        team_id,
        period_start: DateTime::parse_from_rfc3339(&period_start_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        period_end: DateTime::parse_from_rfc3339(&period_end_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        period_type: PeriodType::parse(&period_type_str),
        type_counts: serde_json::from_str(&type_counts_json).unwrap_or_default(),
        type_total_hours: serde_json::from_str(&type_total_hours_json).unwrap_or_default(),
        type_avg_hours: serde_json::from_str(&type_avg_hours_json).unwrap_or_default(),
        total_bottlenecks: total_bottlenecks.max(0) as u32,
        total_hours_lost: total_hours_lost.unwrap_or(0.0),
        avg_bottleneck_duration: avg_bottleneck_duration.unwrap_or(0.0),
        trend_direction: trend_direction_str.map(|s| TrendDirection::parse(&s)).unwrap_or(TrendDirection::Stable),
        trend_percent_change: trend_percent_change.unwrap_or(0.0) as f32,
        computed_at: DateTime::parse_from_rfc3339(&computed_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_intervention_outcome(row: &rusqlite::Row) -> rusqlite::Result<super::aggregation_types::InterventionOutcome> {
    use super::aggregation_types::InterventionOutcome;

    let id_str: String = row.get(0)?;
    let organization_id: String = row.get(1)?;
    let recommendation_id_str: String = row.get(2)?;
    let intervention_type: String = row.get(3)?;
    let intervention_date_str: String = row.get(4)?;
    let outcome_measured_date_str: Option<String> = row.get(5)?;
    let outcome_type: Option<String> = row.get(6)?;
    let outcome_value: Option<f64> = row.get(7)?;
    let pre_metrics_json: Option<String> = row.get(8)?;
    let post_metrics_json: Option<String> = row.get(9)?;
    let confidence_score: f64 = row.get(10)?;
    let learned_pattern_id_str: Option<String> = row.get(11)?;
    let created_at_str: String = row.get(12)?;

    Ok(InterventionOutcome {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        recommendation_id: Uuid::parse_str(&recommendation_id_str).unwrap_or_else(|_| Uuid::new_v4()),
        intervention_type,
        intervention_date: DateTime::parse_from_rfc3339(&intervention_date_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        outcome_measured_date: outcome_measured_date_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)).ok()
        }),
        outcome_type,
        outcome_value,
        pre_intervention_metrics: pre_metrics_json.and_then(|j| serde_json::from_str(&j).ok()),
        post_intervention_metrics: post_metrics_json.and_then(|j| serde_json::from_str(&j).ok()),
        confidence_score: confidence_score as f32,
        learned_pattern_id: learned_pattern_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classification_insert_and_query() {
        let db = AnalyticsDb::in_memory().unwrap();

        let classification = InteractionClassification {
            id: Uuid::new_v4(),
            organization_id: "test-org".to_string(),
            source_type: InteractionSource::TaskComment,
            source_id: "comment-123".to_string(),
            task_id: Some("task-456".to_string()),
            goal_id: None,
            sender_id: "user-789".to_string(),
            content: "Can you clarify the requirements?".to_string(),
            interaction_type: InteractionType::RequestClarification,
            secondary_types: vec![InteractionType::Question],
            confidence_score: 0.92,
            entities: ExtractedEntities::default(),
            sentiment: 0.1,
            urgency_level: UrgencyLevel::Medium,
            references_interaction_id: None,
            original_created_at: Utc::now(),
            classified_at: Utc::now(),
        };

        db.insert_classification(&classification).unwrap();

        let results = db.get_classifications_for_task("task-456").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].interaction_type, InteractionType::RequestClarification);
    }
}
