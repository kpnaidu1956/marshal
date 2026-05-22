import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { analyticsApi } from '@/lib/analyticsApi'
import { useOrgStore } from '@/stores/org'
import { useAuthStore } from '@/stores/auth'
import type { AnalysisJob, PeriodType } from '@/types/analytics'

function useOrgId() {
 return useOrgStore((s) => s.currentOrg?.id ?? '')
}

function useHasAccess() {
 // Analytics visible to all logged-in users
 return !!useAuthStore((s) => s.token)
}

// ============ Teams ============
export function useTeams() {
 const orgId = useOrgId()
 const ok = useHasAccess()
 return useQuery({
 queryKey: ['analytics', 'teams', orgId],
 queryFn: () => analyticsApi.getTeams(orgId),
 enabled: !!orgId && ok,
 staleTime: 10 * 60_000,
 })
}

// ============ Analysis ============
export function useAnalyzeTask() {
 const orgId = useOrgId()
 const qc = useQueryClient()
 return useMutation({
 mutationFn: (taskId: string) => analyticsApi.analyzeTask(taskId, orgId),
 onSuccess: (_d, taskId) => {
 qc.invalidateQueries({ queryKey: ['analytics', 'timeline', 'task', taskId] })
 qc.invalidateQueries({ queryKey: ['analytics', 'interactions', 'task', taskId] })
 },
 })
}

export function useAnalysisJobStatus(jobId: string | null) {
 const orgId = useOrgId()
 return useQuery({
 queryKey: ['analytics', 'job', jobId, orgId],
 queryFn: () => analyticsApi.getJobStatus(jobId!, orgId),
 enabled: !!jobId && !!orgId,
 refetchInterval: (q) => {
 const d = q.state.data as AnalysisJob | undefined
 return d?.status === 'pending' || d?.status === 'processing' ? 2000 : false
 },
 })
}

// ============ Timeline ============
export function useTaskTimeline(taskId: string | null) {
 const orgId = useOrgId()
 return useQuery({
 queryKey: ['analytics', 'timeline', 'task', taskId, orgId],
 queryFn: () => analyticsApi.getTaskTimeline(taskId!, orgId),
 enabled: !!taskId && !!orgId,
 staleTime: 5 * 60_000,
 })
}

// ============ Interactions ============
export function useTaskInteractions(taskId: string | null) {
 const orgId = useOrgId()
 return useQuery({
 queryKey: ['analytics', 'interactions', 'task', taskId, orgId],
 queryFn: () => analyticsApi.getTaskInteractions(taskId!, orgId),
 enabled: !!taskId && !!orgId,
 staleTime: 5 * 60_000,
 })
}

// ============ Aggregations ============
export function useInteractionAggregations(periodType: PeriodType = 'daily') {
 const orgId = useOrgId()
 const ok = useHasAccess()
 return useQuery({
 queryKey: ['analytics', 'aggregations', orgId, periodType],
 queryFn: () => analyticsApi.getInteractionAggregations(orgId, periodType),
 enabled: !!orgId && ok,
 staleTime: 5 * 60_000,
 })
}

export function useTeamAggregations(teamId: string | null, periodType: PeriodType = 'daily', opts?: { enabled?: boolean }) {
 const orgId = useOrgId()
 return useQuery({
 queryKey: ['analytics', 'aggregations', 'team', teamId, orgId, periodType],
 queryFn: () => analyticsApi.getTeamAggregations(teamId!, orgId, periodType),
 enabled: !!teamId && !!orgId && opts?.enabled !== false,
 staleTime: 5 * 60_000,
 })
}

export function useTriggerAggregations() {
 const orgId = useOrgId()
 const qc = useQueryClient()
 return useMutation({
 mutationFn: (periodType: PeriodType) => analyticsApi.triggerAggregations(orgId, periodType),
 onSuccess: () => qc.invalidateQueries({ queryKey: ['analytics', 'aggregations'] }),
 })
}

// ============ Network ============
export function useNetworkGraph(days = 30) {
 const orgId = useOrgId()
 const ok = useHasAccess()
 return useQuery({
 queryKey: ['analytics', 'network', 'graph', orgId, days],
 queryFn: () => analyticsApi.getNetworkGraph(orgId, days),
 enabled: !!orgId && ok,
 staleTime: 10 * 60_000,
 })
}

export function useCrossTeamConnectors() {
 const orgId = useOrgId()
 const ok = useHasAccess()
 return useQuery({
 queryKey: ['analytics', 'network', 'connectors', orgId],
 queryFn: () => analyticsApi.getCrossTeamConnectors(orgId),
 enabled: !!orgId && ok,
 staleTime: 10 * 60_000,
 })
}

// ============ Recommendations ============
export function useOrganizationRecommendations() {
 const orgId = useOrgId()
 const ok = useHasAccess()
 return useQuery({
 queryKey: ['analytics', 'recommendations', 'organization', orgId],
 queryFn: () => analyticsApi.getOrganizationRecommendations(orgId),
 enabled: !!orgId && ok,
 staleTime: 5 * 60_000,
 })
}

// ============ Learning ============
export function useLearningEffectiveness() {
 const orgId = useOrgId()
 const ok = useHasAccess()
 return useQuery({
 queryKey: ['analytics', 'learning', 'effectiveness', orgId],
 queryFn: () => analyticsApi.getLearningEffectiveness(orgId),
 enabled: !!orgId && ok,
 staleTime: 10 * 60_000,
 })
}

export function useApplyLearningAdjustments() {
 const orgId = useOrgId()
 const qc = useQueryClient()
 return useMutation({
 mutationFn: () => analyticsApi.applyLearningAdjustments(orgId),
 onSuccess: () => {
 qc.invalidateQueries({ queryKey: ['analytics', 'patterns'] })
 qc.invalidateQueries({ queryKey: ['analytics', 'recommendations'] })
 },
 })
}
