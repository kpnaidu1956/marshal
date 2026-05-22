import { useState, useEffect, useMemo } from 'react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Users, TrendingUp, BarChart3, RefreshCw, CheckSquare, AlertTriangle, MessageSquare } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import {
 useTeams,
 useTriggerAggregations,
} from '@/hooks/useAnalytics'
import { analyticsApi } from '@/lib/analyticsApi'
import type { PeriodType } from '@/types/analytics'
import {
 BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
 PieChart, Pie, Cell, Legend,
} from 'recharts'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'

interface UserRow {
 id: string
 first_name: string
 last_name: string
 title?: string | null
 manager_id?: string | null
}

interface TaskRow {
 id: string
 title: string
 status: string
 priority?: string
 assigned_to?: string
 due_date?: string
}

interface ComputedTeam {
 manager_id: string
 manager_name: string
 members: UserRow[]
}

interface TeamTaskMetrics {
 manager_id: string
 manager_name: string
 total: number
 assigned: number
 inProgress: number
 completed: number
 high: number
 medium: number
 low: number
}

const STATUS_COLORS: Record<string, string> = {
 Assigned: 'hsl(220, 70%, 55%)',
 'In Progress': 'hsl(45, 93%, 47%)',
 Completed: 'hsl(142, 71%, 45%)',
}

const PRIORITY_COLORS: Record<string, string> = {
 High: 'hsl(0, 84%, 60%)',
 Medium: 'hsl(45, 93%, 47%)',
 Low: 'hsl(142, 71%, 45%)',
}

export function AnalyticsTeamsPage() {
 const [periodType, setPeriodType] = useState<PeriodType>('daily')
 const [selectedTeamId, setSelectedTeamId] = useState<string | null>(null)

 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')
 const { postgrestUrl, apiKey } = detectApiUrls()

 // Analytics hooks (for team selector and refresh)
 const { data: teams, isLoading: teamsLoading } = useTeams()
 const triggerAggregations = useTriggerAggregations()

 // All data: PostgREST + Analytics API loaded together
 const [pgUsers, setPgUsers] = useState<UserRow[]>([])
 const [pgTasks, setPgTasks] = useState<TaskRow[]>([])
 const [aggregations, setAggregations] = useState<import('@/types/analytics').AggregationResponse | null>(null)
 const [loading, setLoading] = useState(false)
 const [loaded, setLoaded] = useState(false)

 const isLoading = loading

 // Fetch all data in one effect
 useEffect(() => {
 if (!orgId || !token || loaded) return
 setLoading(true)
 const client = new PostgRestClient(postgrestUrl, apiKey)

 Promise.all([
 client.get<UserRow>('users',
 new QueryBuilder().select('id,first_name,last_name,title,manager_id').eq('organization_id', orgId).order('first_name', true).limit(200).build(),
 token,
 ).catch(() => [] as UserRow[]),
 client.get<TaskRow>('tasks',
 new QueryBuilder().select('id,title,status,priority,assigned_to,due_date').eq('organization_id', orgId).limit(5000).build(),
 token,
 ).catch(() => [] as TaskRow[]),
 analyticsApi.getInteractionAggregations(orgId, periodType).catch(() => null),
 ]).then(([users, tasks, agg]) => {
 setPgUsers(users)
 setPgTasks(tasks)
 if (agg) setAggregations(agg)
 setLoaded(true)
 }).finally(() => setLoading(false))
 }, [orgId, token, loaded, periodType, postgrestUrl, apiKey])

 const analyticsHasData = !!(aggregations?.interaction_types?.length)

 // Build teams from users (group by manager_id)
 const computedTeams = useMemo((): ComputedTeam[] => {
 if (!pgUsers.length) return []
 const byManager = new Map<string, UserRow[]>()
 for (const u of pgUsers) {
 const mid = u.manager_id || '__no_manager__'
 const list = byManager.get(mid) || []
 list.push(u)
 byManager.set(mid, list)
 }
 const result: ComputedTeam[] = []
 for (const [mid, members] of byManager) {
 if (mid === '__no_manager__') continue
 const mgr = pgUsers.find((u) => u.id === mid)
 const name = mgr ? `${mgr.first_name} ${mgr.last_name}` : 'Unknown Manager'
 result.push({ manager_id: mid, manager_name: name, members })
 }
 // Also include users who are managers but have no manager themselves
 const noMgrGroup = byManager.get('__no_manager__') || []
 if (noMgrGroup.length > 0 && result.length === 0) {
 result.push({ manager_id: '__unassigned__', manager_name: 'Unassigned', members: noMgrGroup })
 }
 return result.sort((a, b) => b.members.length - a.members.length)
 }, [pgUsers])

 // Effective teams list: prefer analytics API, fall back to computed
 const effectiveTeams = useMemo(() => {
 if (teams?.length) return teams
 return computedTeams.map((t) => ({ manager_id: t.manager_id, manager_name: t.manager_name }))
 }, [teams, computedTeams])

 // Team task metrics
 const teamMetrics = useMemo((): TeamTaskMetrics[] => {
 if (!computedTeams.length || !pgTasks.length) return []
 return computedTeams.map((team) => {
 const memberIds = new Set(team.members.map((m) => m.id))
 // Also include the manager
 memberIds.add(team.manager_id)
 const teamTasks = pgTasks.filter((t) => t.assigned_to && memberIds.has(t.assigned_to))
 return {
 manager_id: team.manager_id,
 manager_name: team.manager_name,
 total: teamTasks.length,
 assigned: teamTasks.filter((t) => t.status === 'Assigned').length,
 inProgress: teamTasks.filter((t) => t.status === 'In Progress').length,
 completed: teamTasks.filter((t) => t.status === 'Completed').length,
 high: teamTasks.filter((t) => t.priority === 'High').length,
 medium: teamTasks.filter((t) => t.priority === 'Medium').length,
 low: teamTasks.filter((t) => t.priority === 'Low').length,
 }
 }).filter((m) => m.total > 0)
 }, [computedTeams, pgTasks])

 // Filtered metrics by selected team
 const filteredMetrics = useMemo(() => {
 if (!selectedTeamId) return teamMetrics
 return teamMetrics.filter((m) => m.manager_id === selectedTeamId)
 }, [teamMetrics, selectedTeamId])

 // Chart data: task counts per team (bar chart)
 const taskCountsChart = useMemo(() => {
 return filteredMetrics.map((m) => ({
 team: m.manager_name.length > 15 ? m.manager_name.slice(0, 14) + '...' : m.manager_name,
 Assigned: m.assigned,
 'In Progress': m.inProgress,
 Completed: m.completed,
 }))
 }, [filteredMetrics])

 // Chart data: priority breakdown (pie chart) — aggregated across filtered teams
 const priorityChart = useMemo(() => {
 const totals = { High: 0, Medium: 0, Low: 0 }
 for (const m of filteredMetrics) {
 totals.High += m.high
 totals.Medium += m.medium
 totals.Low += m.low
 }
 return Object.entries(totals)
 .filter(([, v]) => v > 0)
 .map(([name, value]) => ({ name, value, color: PRIORITY_COLORS[name] || 'hsl(220,9%,46%)' }))
 }, [filteredMetrics])

 // Interaction type chart data from aggregations
 const INTERACTION_COLORS: Record<string, string> = {
 feedback: 'hsl(221, 83%, 53%)',
 status_update: 'hsl(142, 71%, 45%)',
 question: 'hsl(45, 93%, 47%)',
 assignment: 'hsl(262, 83%, 58%)',
 blocker: 'hsl(0, 84%, 60%)',
 acknowledgment: 'hsl(190, 90%, 50%)',
 other: 'hsl(220, 9%, 46%)',
 escalation: 'hsl(330, 81%, 60%)',
 direction: 'hsl(25, 95%, 53%)',
 recognition: 'hsl(160, 60%, 45%)',
 request_approval: 'hsl(280, 60%, 55%)',
 }

 const interactionTypeChart = useMemo(() => {
 if (!aggregations?.interaction_types?.length) return []
 const totals = new Map<string, number>()
 for (const it of aggregations.interaction_types) {
 totals.set(it.interaction_type, (totals.get(it.interaction_type) || 0) + it.count)
 }
 return Array.from(totals.entries())
 .sort((a, b) => b[1] - a[1])
 .map(([name, value]) => ({
 name: name.split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '),
 value,
 color: INTERACTION_COLORS[name] || 'hsl(220, 9%, 46%)',
 }))
 }, [aggregations])

 const totalInteractions = useMemo(() => {
 return interactionTypeChart.reduce((s, e) => s + e.value, 0)
 }, [interactionTypeChart])

 // Summary stats
 const totalTeams = effectiveTeams.length
 const totalTasks = filteredMetrics.reduce((s, m) => s + m.total, 0)
 const completionRate = totalTasks > 0
 ? ((filteredMetrics.reduce((s, m) => s + m.completed, 0) / totalTasks) * 100).toFixed(1)
 : '0.0'

 const usingFallback = !analyticsHasData && loaded

 const handleRefresh = () => {
 triggerAggregations.mutate(periodType)
 setLoaded(false)
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold">Team Analytics</h1>
 <p className="text-muted-foreground text-sm">
 View team performance metrics and task distribution.
 </p>
 </div>
 <div className="flex items-center gap-3">
 <Select value={periodType} onValueChange={(v) => setPeriodType(v as PeriodType)}>
 <SelectTrigger className="w-[120px]"><SelectValue /></SelectTrigger>
 <SelectContent>
 <SelectItem value="daily">Daily</SelectItem>
 <SelectItem value="weekly">Weekly</SelectItem>
 <SelectItem value="monthly">Monthly</SelectItem>
 </SelectContent>
 </Select>
 <Select value={selectedTeamId || 'all'} onValueChange={(v) => setSelectedTeamId(v === 'all' ? null : v)}>
 <SelectTrigger className="w-[180px]"><SelectValue placeholder="All Teams" /></SelectTrigger>
 <SelectContent>
 <SelectItem value="all">All Teams</SelectItem>
 {effectiveTeams.map((team) => (
 <SelectItem key={team.manager_id} value={team.manager_id}>
 {team.manager_name}&apos;s Team
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 <Button variant="outline" size="icon" onClick={handleRefresh} disabled={triggerAggregations.isPending}>
 <RefreshCw className={`h-4 w-4 ${triggerAggregations.isPending ? 'animate-spin' : ''}`} />
 </Button>
 </div>
 </div>

 {/* fallback banner removed — PostgREST metrics are the primary view */}

 {/* Stats Cards */}
 <div className="grid gap-6 md:grid-cols-4">
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Teams</CardTitle>
 <Users className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-8 w-16" /> : (
 <><div className="text-2xl font-bold">{totalTeams}</div>
 <p className="text-xs text-muted-foreground">Active teams in organization</p></>
 )}
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Total Tasks</CardTitle>
 <CheckSquare className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-8 w-24" /> : (
 <><div className="text-2xl font-bold">{totalTasks}</div>
 <p className="text-xs text-muted-foreground">Across {selectedTeamId ? 'selected team' : 'all teams'}</p></>
 )}
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Completion Rate</CardTitle>
 <TrendingUp className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-8 w-16" /> : (
 <><div className="text-2xl font-bold">{completionRate}%</div>
 <p className="text-xs text-muted-foreground">Tasks completed</p></>
 )}
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Interactions</CardTitle>
 <MessageSquare className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-8 w-16" /> : (
 <><div className="text-2xl font-bold">{totalInteractions.toLocaleString()}</div>
 <p className="text-xs text-muted-foreground">Classified communications</p></>
 )}
 </CardContent>
 </Card>
 </div>

 {/* Team Cards */}
 {filteredMetrics.length > 0 && (
 <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
 {filteredMetrics.map((m) => {
 const team = computedTeams.find((t) => t.manager_id === m.manager_id)
 return (
 <Card key={m.manager_id}>
 <CardHeader className="pb-2">
 <CardTitle className="text-base flex items-center gap-2">
 <Users className="h-4 w-4" />
 {m.manager_name}&apos;s Team
 </CardTitle>
 <CardDescription className="text-xs">
 {team?.members.length || 0} members &middot; {m.total} tasks
 </CardDescription>
 </CardHeader>
 <CardContent className="space-y-2">
 <div className="flex flex-wrap gap-1.5">
 <Badge variant="secondary" className="text-xs">{m.assigned} Assigned</Badge>
 <Badge className="bg-primary/10 text-primary text-xs">{m.inProgress} In Progress</Badge>
 <Badge className="bg-emerald-500/10 text-emerald-600 text-xs">{m.completed} Completed</Badge>
 </div>
 <div className="flex flex-wrap gap-1.5">
 {m.high > 0 && <Badge variant="destructive" className="text-xs">{m.high} High</Badge>}
 {m.medium > 0 && <Badge className="bg-amber-500/10 text-amber-600 text-xs">{m.medium} Medium</Badge>}
 {m.low > 0 && <Badge className="bg-emerald-500/10 text-emerald-600 text-xs">{m.low} Low</Badge>}
 </div>
 {/* Inline progress bar */}
 {m.total > 0 && (
 <div className="w-full h-2 bg-muted rounded-full overflow-hidden flex">
 <div className="h-full bg-emerald-500" style={{ width: `${(m.completed / m.total) * 100}%` }} />
 <div className="h-full bg-primary" style={{ width: `${(m.inProgress / m.total) * 100}%` }} />
 <div className="h-full bg-muted-foreground/30" style={{ width: `${(m.assigned / m.total) * 100}%` }} />
 </div>
 )}
 </CardContent>
 </Card>
 )
 })}
 </div>
 )}

 {/* Interaction Types Chart */}
 {interactionTypeChart.length > 0 && (
 <Card>
 <CardHeader>
 <CardTitle className="flex items-center gap-2">
 <MessageSquare className="h-5 w-5" />
 Interaction Types
 </CardTitle>
 <CardDescription>Distribution of classified communication types ({periodType})</CardDescription>
 </CardHeader>
 <CardContent className="h-[300px]">
 <ResponsiveContainer width="100%" height="100%">
 <BarChart data={interactionTypeChart} layout="vertical" margin={{ left: 20 }}>
 <CartesianGrid strokeDasharray="3 3" />
 <XAxis type="number" tick={{ fontSize: 11 }} />
 <YAxis type="category" dataKey="name" tick={{ fontSize: 11 }} width={110} />
 <Tooltip formatter={(v) => [Number(v).toLocaleString(), 'Count']} />
 <Bar dataKey="value" radius={[0, 4, 4, 0]}>
 {interactionTypeChart.map((entry, i) => <Cell key={i} fill={entry.color} />)}
 </Bar>
 </BarChart>
 </ResponsiveContainer>
 </CardContent>
 </Card>
 )}

 {/* Charts Row */}
 <div className="grid gap-6 md:grid-cols-2">
 <Card>
 <CardHeader>
 <CardTitle className="flex items-center gap-2">
 <BarChart3 className="h-5 w-5" />
 Task Status by Team
 </CardTitle>
 <CardDescription>Distribution of task statuses across teams</CardDescription>
 </CardHeader>
 <CardContent className="h-[350px]">
 {isLoading ? <Skeleton className="h-[280px] w-full" /> :
 taskCountsChart.length > 0 ? (
 <ResponsiveContainer width="100%" height="100%">
 <BarChart data={taskCountsChart}>
 <CartesianGrid strokeDasharray="3 3" />
 <XAxis dataKey="team" tick={{ fontSize: 11 }} angle={-20} textAnchor="end" height={60} />
 <YAxis tick={{ fontSize: 12 }} />
 <Tooltip />
 <Legend wrapperStyle={{ fontSize: '12px' }} />
 <Bar dataKey="Assigned" fill={STATUS_COLORS['Assigned']} stackId="status" radius={[0, 0, 0, 0]} />
 <Bar dataKey="In Progress" fill={STATUS_COLORS['In Progress']} stackId="status" />
 <Bar dataKey="Completed" fill={STATUS_COLORS['Completed']} stackId="status" radius={[4, 4, 0, 0]} />
 </BarChart>
 </ResponsiveContainer>
 ) : <div className="h-full flex items-center justify-center text-muted-foreground">No task data available</div>}
 </CardContent>
 </Card>

 <Card>
 <CardHeader>
 <CardTitle>Priority Breakdown</CardTitle>
 <CardDescription>Task distribution by priority level</CardDescription>
 </CardHeader>
 <CardContent className="h-[350px]">
 {isLoading ? <Skeleton className="h-[250px] w-[250px] rounded-full mx-auto" /> :
 priorityChart.length > 0 ? (
 <ResponsiveContainer width="100%" height="100%">
 <PieChart>
 <Pie data={priorityChart} dataKey="value" nameKey="name" cx="50%" cy="50%"
 innerRadius={60} outerRadius={100} paddingAngle={2}
 label={({ name, percent }) => `${name} ${((percent ?? 0) * 100).toFixed(0)}%`} labelLine={false}>
 {priorityChart.map((entry, i) => <Cell key={i} fill={entry.color} />)}
 </Pie>
 <Tooltip formatter={(v, name) => [v, name]} />
 <Legend wrapperStyle={{ fontSize: '12px' }} />
 </PieChart>
 </ResponsiveContainer>
 ) : <div className="h-full flex items-center justify-center text-muted-foreground">No priority data available</div>}
 </CardContent>
 </Card>
 </div>
 </div>
 )
}
