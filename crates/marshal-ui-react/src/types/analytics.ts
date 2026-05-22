// Analytics API Types — adapted from FD-NEW

// ============ Enums ============
export type InteractionType =
 | 'request_clarification' | 'request_resources' | 'direction'
 | 'suggestion' | 'request_approval' | 'status_update'
 | 'acknowledgment' | 'escalation' | 'blocker'
 | 'question' | 'answer' | 'assignment'
 | 'feedback' | 'recognition' | 'other';

export type UrgencyLevel = 'low' | 'medium' | 'high' | 'critical';
export type PatternType = 'success' | 'failure' | 'bottleneck' | 'efficiency';
export type RecommendationType = 'process' | 'communication' | 'resource' | 'timing';
export type JobStatus = 'pending' | 'processing' | 'completed' | 'failed';
export type RecommendationStatus = 'pending' | 'accepted' | 'rejected' | 'implemented';
export type EntityType = 'task' | 'goal';
export type TargetType = 'task' | 'goal' | 'team' | 'organization';
export type SourceType = 'task_comment' | 'goal_comment' | 'message' | 'activity_log';
export type PeriodType = 'daily' | 'weekly' | 'monthly';
export type TrendDirection = 'improving' | 'declining' | 'worsening' | 'stable';
export type BottleneckType = 'approval_delay' | 'blocked_period' | 'communication_gap' | 'clarification_loop';
export type InterventionType = 'process_change' | 'resource_allocation' | 'communication_improvement' | 'workflow_adjustment';
export type OutcomeType = 'bottleneck_reduction' | 'cycle_time_improvement' | 'sentiment_improvement' | 'participation_increase';

// ============ Core Types ============
export interface WorkflowPhase {
 name: string;
 start: string;
 end: string | null;
 interaction_count: number;
 participants: string[];
}

export interface TimelineEvent {
 timestamp: string;
 event_type: string;
 description: string;
 actor_id: string;
 actor_name?: string;
 interaction_id?: string;
 metadata?: Record<string, unknown>;
}

export interface WorkflowBottleneck {
 bottleneck_type: string;
 duration_hours: number;
 description: string;
 start: string;
 end: string;
 caused_by?: string;
}

export interface WorkflowTimeline {
 id: string;
 entity_type: EntityType;
 entity_id: string;
 total_interactions: number;
 total_participants: number;
 total_duration_hours: number | null;
 phases: WorkflowPhase[];
 key_events: TimelineEvent[];
 bottlenecks: WorkflowBottleneck[];
 status: string;
 opened_at: string;
 closed_at: string | null;
}

export interface InteractionClassification {
 id: string;
 source_type: SourceType;
 source_id: string;
 task_id?: string;
 goal_id?: string;
 sender_id: string;
 content: string;
 interaction_type: InteractionType;
 secondary_types: InteractionType[];
 confidence_score: number;
 sentiment: number;
 urgency_level: UrgencyLevel;
 entities: {
 mentioned_users: string[];
 mentioned_deadlines: string[];
 action_items: string[];
 blockers: string[];
 resources: string[];
 };
 original_created_at: string;
}

export interface WorkflowPattern {
 id: string;
 pattern_type: PatternType;
 pattern_name: string;
 description: string;
 criteria: Record<string, unknown>;
 occurrence_count: number;
 success_correlation: number | null;
 avg_time_impact_hours: number | null;
 confidence_score: number;
 examples: string[];
 is_active: boolean;
}

export interface EfficiencyRecommendation {
 id: string;
 target_type: TargetType;
 target_id?: string;
 recommendation_type: RecommendationType;
 title: string;
 description: string;
 suggested_actions: string[];
 based_on_patterns: string[];
 evidence: Record<string, unknown>;
 priority: 'low' | 'medium' | 'high';
 estimated_time_savings_hours: number | null;
 status: RecommendationStatus;
}

export interface AnalysisJob {
 job_id: string;
 status: JobStatus;
 progress_percent: number;
 current_stage: string;
 entity_type: EntityType;
 entity_id: string;
 started_at: string;
 completed_at: string | null;
 error_message: string | null;
}

// ============ Team Types ============
export interface TeamMember {
 user_id: string;
 user_name?: string;
 role?: string;
}

export interface Team {
 manager_id: string;
 manager_name: string;
 member_ids: string[];
 members?: TeamMember[];
}

// ============ Aggregation Types ============
export interface InteractionTypeAggregation {
 period_start: string;
 period_end: string;
 period_type: PeriodType;
 interaction_type: InteractionType;
 count: number;
 avg_sentiment: number;
 avg_urgency_score: number;
}

export interface SentimentAggregation {
 period_start: string;
 period_end: string;
 period_type: PeriodType;
 avg_sentiment: number;
 sentiment_std_dev: number;
 positive_count: number;
 neutral_count: number;
 negative_count: number;
 rolling_avg_7d: number | null;
 rolling_avg_30d: number | null;
 trend_direction: TrendDirection;
}

export interface BottleneckAggregation {
 period_start: string;
 period_end: string;
 period_type: PeriodType;
 bottleneck_type: BottleneckType;
 occurrence_count: number;
 total_duration_hours: number;
 avg_duration_hours: number;
 trend_direction: TrendDirection;
}

export interface AggregationResponse {
 organization_id: string;
 team_id?: string;
 period_type: PeriodType;
 interaction_types: InteractionTypeAggregation[];
 sentiment: SentimentAggregation[];
 bottlenecks: BottleneckAggregation[];
}

// ============ Network Types ============
export interface ParticipationEdge {
 source_user_id: string;
 target_user_id: string;
 interaction_count: number;
 avg_sentiment: number;
 last_interaction_at: string;
}

export interface ParticipationMetrics {
 user_id: string;
 user_name?: string;
 degree_centrality: number;
 betweenness_centrality: number;
 closeness_centrality: number;
 total_interactions: number;
}

export interface NetworkGraph {
 nodes: ParticipationMetrics[];
 edges: ParticipationEdge[];
 computed_at: string;
}

export interface CrossTeamConnector {
 user_id: string;
 user_name?: string;
 teams_connected: string[];
 bridge_score: number;
}

// ============ Learning Types ============
export interface LearningEffectiveness {
 intervention_type: InterventionType;
 total_interventions: number;
 successful_interventions: number;
 success_rate: number;
 avg_improvement: number;
}

// ============ User Analytics Types ============
export interface UserPerformanceResponse {
 user_id: string;
 organization_id: string;
 period: { from: string; to: string };
 tasks: { total: number; completed: number; by_status: Record<string, number> };
 goals: { total: number; completed: number; by_status: Record<string, number> };
 completion_rate: number;
}

export interface UserInteractionsResponse {
 user_id: string;
 organization_id: string;
 period: { days: number; from: string; to: string };
 summary: { total_interactions: number; average_sentiment: number; unique_collaborators: number };
 interaction_types: Record<string, number>;
 top_collaborators: Array<{ user_id: string; interaction_count: number }>;
}

export interface SentimentTimeSeriesPoint {
 date: string;
 average_sentiment: number;
 interaction_count: number;
}

export interface UserSentimentResponse {
 user_id: string;
 organization_id: string;
 period: { from: string; to: string };
 overall: { average_sentiment: number; total_interactions: number };
 time_series: SentimentTimeSeriesPoint[];
}

// ============ Color mappings ============
export const INTERACTION_COLORS: Record<InteractionType, string> = {
 blocker: 'hsl(0, 84%, 60%)',
 escalation: 'hsl(0, 84%, 60%)',
 request_approval: 'hsl(25, 95%, 53%)',
 request_resources: 'hsl(32, 95%, 44%)',
 question: 'hsl(48, 96%, 53%)',
 request_clarification: 'hsl(45, 93%, 47%)',
 status_update: 'hsl(221, 83%, 53%)',
 acknowledgment: 'hsl(217, 91%, 60%)',
 direction: 'hsl(263, 70%, 50%)',
 assignment: 'hsl(280, 65%, 60%)',
 suggestion: 'hsl(142, 71%, 45%)',
 feedback: 'hsl(142, 76%, 36%)',
 answer: 'hsl(199, 89%, 48%)',
 recognition: 'hsl(156, 72%, 67%)',
 other: 'hsl(220, 9%, 46%)',
};

export const TREND_COLORS: Record<TrendDirection, string> = {
 improving: 'hsl(142, 71%, 45%)',
 declining: 'hsl(45, 93%, 47%)',
 worsening: 'hsl(0, 84%, 60%)',
 stable: 'hsl(220, 9%, 46%)',
};
