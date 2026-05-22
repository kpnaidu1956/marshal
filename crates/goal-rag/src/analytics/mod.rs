//! Interaction Analytics & Workflow Intelligence System
//!
//! This module provides:
//! - Classification of team communications (comments, messages)
//! - Workflow timeline reconstruction
//! - Pattern learning from successful/failed workflows
//! - Efficiency recommendations
//! - Team and organization-level aggregations (Phase 6)
//! - Participation network and centrality metrics
//! - Intervention tracking and outcome learning

pub mod types;
pub mod storage;
pub mod classifier;
pub mod timeline;
pub mod pattern_learner;
pub mod recommender;
pub mod jobs;

// Phase 6: Team & Organization Aggregations
pub mod aggregation_types;
pub mod team_manager;
pub mod aggregator;
pub mod network;
pub mod learning;

pub use types::*;
pub use storage::AnalyticsDb;
pub use classifier::OllamaClassifier;
pub use aggregation_types::*;
pub use team_manager::{TeamManager, TeamInfo, SyncResult};
pub use aggregator::{Aggregator, AggregationResult, AllTeamsAggregationResult};
pub use network::{NetworkAnalyzer, ParticipationNetwork, ConnectorInfo};
pub use learning::{LearningSystem, EffectivenessAnalysis, OrganizationLearning};
