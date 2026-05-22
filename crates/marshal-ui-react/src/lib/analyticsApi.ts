import { useAuthStore } from '@/stores/auth'
import { detectApiUrls } from '@/lib/config'
import type {
 AggregationResponse,
 AnalysisJob,
 CrossTeamConnector,
 EfficiencyRecommendation,
 InteractionClassification,
 InteractionTypeAggregation,
 LearningEffectiveness,
 NetworkGraph,
 ParticipationMetrics,
 PeriodType,
 Team,
 UserInteractionsResponse,
 UserPerformanceResponse,
 UserSentimentResponse,
 WorkflowPattern,
 WorkflowTimeline,
} from '@/types/analytics'

/**
 * Direct fetch to RAG server analytics endpoints.
 * Auth from Zustand store (getState works outside React).
 */
async function analyticsRequest<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
 const { ragUrl, apiKey } = detectApiUrls()
 const token = useAuthStore.getState().token

 const headers: Record<string, string> = { 'Content-Type': 'application/json' }
 if (token) headers['Authorization'] = `Bearer ${token}`
 if (apiKey) headers['apikey'] = apiKey

 const res = await fetch(`${ragUrl}/api/analytics${path}`, {
 method,
 headers,
 body: body ? JSON.stringify(body) : undefined,
 signal: AbortSignal.timeout(15_000),
 })

 if (!res.ok) {
 const text = await res.text().catch(() => '')
 throw new Error(text || `Analytics API error: ${res.status}`)
 }

 return res.json() as Promise<T>
}

export const analyticsApi = {
 // Teams
 getTeams: (orgId: string) =>
 analyticsRequest<Team[]>(`/teams?org=${encodeURIComponent(orgId)}`),

 // Analysis
 analyzeTask: (taskId: string, orgId: string) =>
 analyticsRequest<AnalysisJob>(`/analysis/task/${taskId}`, 'POST', { organization_id: orgId }),

 getJobStatus: (jobId: string, orgId: string) =>
 analyticsRequest<AnalysisJob>(`/jobs/${jobId}?organization_id=${encodeURIComponent(orgId)}`),

 // Timeline
 getTaskTimeline: (taskId: string, orgId: string) =>
 analyticsRequest<WorkflowTimeline>(`/timeline/task/${taskId}?organization_id=${encodeURIComponent(orgId)}`),

 // Interactions
 getTaskInteractions: (taskId: string, orgId: string) =>
 analyticsRequest<{ interactions: InteractionClassification[] }>(`/interactions/task/${taskId}?organization_id=${encodeURIComponent(orgId)}`)
 .then((r) => r.interactions ?? []),

 // Aggregations
 getInteractionAggregations: (orgId: string, periodType: PeriodType = 'daily') =>
 analyticsRequest<RawAggregationRow[]>(`/interactions/aggregate?org=${encodeURIComponent(orgId)}&period=${periodType}`)
 .then((rows) => transformAggregationRows(rows, orgId, periodType)),

 getTeamAggregations: (teamId: string, orgId: string, periodType: PeriodType = 'daily') =>
 analyticsRequest<RawAggregationRow[]>(`/interactions/aggregate/team/${teamId}?org=${encodeURIComponent(orgId)}&period=${periodType}`)
 .then((rows) => transformAggregationRows(rows, orgId, periodType, teamId)),

 triggerAggregations: (orgId: string, periodType: PeriodType) =>
 analyticsRequest<{ job_id: string }>('/aggregations/trigger', 'POST', { organization_id: orgId, period_type: periodType }),

 // Network
 getNetworkGraph: (orgId: string, days = 30) =>
 analyticsRequest<RawNetworkGraph>(`/network/graph?org=${encodeURIComponent(orgId)}&days=${days}`)
 .then((raw) => transformNetworkGraph(raw)),

 getCrossTeamConnectors: (orgId: string) =>
 analyticsRequest<CrossTeamConnector[]>(`/network/connectors?org=${encodeURIComponent(orgId)}`),

 // Patterns
 getPatterns: (orgId: string) =>
 analyticsRequest<WorkflowPattern[]>(`/patterns?organization_id=${encodeURIComponent(orgId)}`),

 // Recommendations
 getOrganizationRecommendations: (orgId: string) =>
 analyticsRequest<{ recommendations: EfficiencyRecommendation[] }>(`/recommendations/organization?organization_id=${encodeURIComponent(orgId)}`)
 .then((r) => r.recommendations ?? []),

 // Learning
 getLearningEffectiveness: (orgId: string) =>
 analyticsRequest<RawLearningEffectiveness>(`/learning/effectiveness?org=${encodeURIComponent(orgId)}`)
 .then((raw) => transformLearningEffectiveness(raw)),

 applyLearningAdjustments: (orgId: string) =>
 analyticsRequest<{ adjustments_applied: number }>('/learning/adjust', 'POST', { organization_id: orgId }),

 // User Analytics
 getUserPerformance: (userId: string, orgId: string, fromDate?: string, toDate?: string) => {
 const params = new URLSearchParams({ organization_id: orgId })
 if (fromDate) params.append('from_date', fromDate)
 if (toDate) params.append('to_date', toDate)
 return analyticsRequest<UserPerformanceResponse>(`/user/${userId}/performance?${params}`)
 },

 getUserInteractions: (userId: string, orgId: string, days = 30) =>
 analyticsRequest<UserInteractionsResponse>(`/user/${userId}/interactions?organization_id=${encodeURIComponent(orgId)}&days=${days}`),

 getUserSentiment: (userId: string, orgId: string, fromDate?: string, toDate?: string) => {
 const params = new URLSearchParams({ organization_id: orgId })
 if (fromDate) params.append('from_date', fromDate)
 if (toDate) params.append('to_date', toDate)
 return analyticsRequest<UserSentimentResponse>(`/user/${userId}/sentiment?${params}`)
 },
}

// ============ Raw server response types ============

interface RawAggregationRow {
 id: string
 organization_id: string
 period_start: string
 period_end: string
 period_type: PeriodType
 type_counts: Record<string, number>
 total_interactions: number
 clarification_ratio: number
 blocker_ratio: number
 escalation_ratio: number
 computed_at: string
}

interface RawNetworkGraph {
 nodes: string[] | ParticipationMetrics[]
 edges: NetworkGraph['edges']
 organization_id?: string
 period_start?: string
 period_end?: string
 computed_at?: string
}

interface RawLearningEffectiveness {
 organization_id: string
 total_recommendations: number
 implemented_count: number
 accepted_count: number
 rejected_count: number
 adoption_rate: number
 total_patterns_learned: number
 high_confidence_patterns: number
 learning_velocity: number
 computed_at: string
}

function transformAggregationRows(
 rows: RawAggregationRow[],
 orgId: string,
 periodType: PeriodType,
 teamId?: string,
): AggregationResponse {
 const interactionTypes: InteractionTypeAggregation[] = []
 for (const row of rows) {
 if (row.type_counts && typeof row.type_counts === 'object') {
 for (const [type, count] of Object.entries(row.type_counts)) {
 interactionTypes.push({
 period_start: row.period_start,
 period_end: row.period_end,
 period_type: row.period_type || periodType,
 interaction_type: type as InteractionTypeAggregation['interaction_type'],
 count: count as number,
 avg_sentiment: 0,
 avg_urgency_score: 0,
 })
 }
 }
 }
 return {
 organization_id: orgId,
 team_id: teamId,
 period_type: periodType,
 interaction_types: interactionTypes,
 sentiment: [],
 bottlenecks: [],
 }
}

function transformNetworkGraph(raw: RawNetworkGraph): NetworkGraph {
 let nodes: ParticipationMetrics[]
 if (raw.nodes?.length && typeof raw.nodes[0] === 'string') {
 nodes = (raw.nodes as string[]).map((userId) => ({
 user_id: userId,
 user_name: undefined,
 degree_centrality: 0,
 betweenness_centrality: 0,
 closeness_centrality: 0,
 total_interactions: 0,
 }))
 } else {
 nodes = (raw.nodes as ParticipationMetrics[]) || []
 }
 return {
 nodes,
 edges: raw.edges || [],
 computed_at: raw.computed_at || new Date().toISOString(),
 }
}

function transformLearningEffectiveness(raw: RawLearningEffectiveness): LearningEffectiveness[] {
 // Server returns a summary object, not per-intervention-type data.
 // Synthesize meaningful entries from available data.
 if (!raw || !raw.total_recommendations) return []
 const total = raw.total_recommendations
 const entries: LearningEffectiveness[] = []
 if (raw.implemented_count > 0 || raw.accepted_count > 0 || raw.rejected_count > 0) {
 entries.push({
 intervention_type: 'process_change',
 total_interventions: total,
 successful_interventions: raw.implemented_count,
 success_rate: raw.adoption_rate,
 avg_improvement: raw.learning_velocity,
 })
 if (raw.accepted_count > 0) {
 entries.push({
 intervention_type: 'workflow_adjustment',
 total_interventions: raw.accepted_count + raw.rejected_count,
 successful_interventions: raw.accepted_count,
 success_rate: raw.accepted_count / Math.max(1, raw.accepted_count + raw.rejected_count),
 avg_improvement: raw.learning_velocity,
 })
 }
 } else {
 // No action taken yet — show summary as single entry
 entries.push({
 intervention_type: 'process_change',
 total_interventions: total,
 successful_interventions: raw.implemented_count,
 success_rate: raw.adoption_rate,
 avg_improvement: raw.learning_velocity,
 })
 }
 return entries
}

// Utility functions
export function formatInteractionType(type: string): string {
 return type.split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')
}

export function formatDuration(hours: number): string {
 if (hours < 1) return `${Math.round(hours * 60)}m`
 if (hours < 24) return `${hours.toFixed(1)}h`
 const days = Math.floor(hours / 24)
 const rem = hours % 24
 return `${days}d ${rem.toFixed(0)}h`
}

export function formatSentiment(sentiment: number): { label: string; color: string } {
 if (sentiment >= 0.3) return { label: 'Positive', color: 'hsl(142, 71%, 45%)' }
 if (sentiment <= -0.3) return { label: 'Negative', color: 'hsl(0, 84%, 60%)' }
 return { label: 'Neutral', color: 'hsl(220, 9%, 46%)' }
}

export function formatTrendDirection(trend: string): { label: string; color: string; icon: string } {
 switch (trend) {
 case 'improving': return { label: 'Improving', color: 'hsl(142, 71%, 45%)', icon: '\u2191' }
 case 'declining': return { label: 'Declining', color: 'hsl(45, 93%, 47%)', icon: '\u2198' }
 case 'worsening': return { label: 'Worsening', color: 'hsl(0, 84%, 60%)', icon: '\u2193' }
 default: return { label: 'Stable', color: 'hsl(220, 9%, 46%)', icon: '\u2192' }
 }
}
