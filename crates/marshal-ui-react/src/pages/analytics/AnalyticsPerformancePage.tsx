import { useState, useEffect, useMemo } from 'react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import { Tooltip as UITooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { User, Target, CheckSquare, Users, MessageSquare, AlertTriangle, Clock, CalendarDays, ThumbsUp, Flame } from 'lucide-react'
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine } from 'recharts'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { analyticsApi } from '@/lib/analyticsApi'
import type { UserPerformanceResponse, UserInteractionsResponse, UserSentimentResponse } from '@/types/analytics'

interface UserOption {
 id: string
 first_name: string
 last_name: string
 title?: string | null
 manager_id?: string | null
}

interface TaskRow { id: string; title: string; status: string; due_date: string; priority?: string; assigned_to?: string }
interface GoalRow { id: string; title: string; status?: string; target_date?: string; created_by?: string }
interface CommentRow { id: string; task_id: string; author_id: string; content: string; created_at: string; is_private?: boolean }
interface TimeEntryRow { id: string; employee_id: string; pay_code: string; hours: number; entry_date: string }

function formatDate(d: Date) {
 return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

const PRIORITY_COLORS: Record<string, string> = {
 Critical: 'bg-red-500/10 text-red-600',
 High: 'bg-orange-500/10 text-orange-600',
 Medium: 'bg-yellow-500/10 text-yellow-700',
 Low: 'bg-blue-500/10 text-blue-600',
}

export function AnalyticsPerformancePage() {
 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [selectedUserId, setSelectedUserId] = useState('')
 const [days, setDays] = useState(30)
 const [users, setUsers] = useState<UserOption[]>([])
 const [, setUsersLoading] = useState(true)

 // API data
 const [performanceData, setPerformanceData] = useState<UserPerformanceResponse | null>(null)
 const [interactionsData, setInteractionsData] = useState<UserInteractionsResponse | null>(null)
 const [sentimentData, setSentimentData] = useState<UserSentimentResponse | null>(null)
 const [tasksFromDb, setTasksFromDb] = useState<TaskRow[]>([])
 const [goalsFromDb, setGoalsFromDb] = useState<GoalRow[]>([])
 const [kudosComments, setKudosComments] = useState<CommentRow[]>([])
 const [timeEntries, setTimeEntries] = useState<TimeEntryRow[]>([])
 const [loading, setLoading] = useState(false)

 const toDate = formatDate(new Date())
 const fromDate = formatDate(new Date(Date.now() - days * 86_400_000))

 // Load users
 useEffect(() => {
 if (!orgId) { setUsersLoading(false); return }
 const client = new PostgRestClient(postgrestUrl, apiKey)
 client.get<UserOption>('users',
 new QueryBuilder().select('id,first_name,last_name,title,manager_id').eq('organization_id', orgId).order('first_name', true).limit(100).build(),
 token,
 ).then(setUsers).catch(() => {}).finally(() => setUsersLoading(false))
 }, [orgId, token, postgrestUrl, apiKey])

 const userMap = useMemo(() => {
 const m: Record<string, string> = {}
 for (const u of users) m[u.id] = `${u.first_name} ${u.last_name}`
 return m
 }, [users])

 // Load data when user selected
 useEffect(() => {
 if (!selectedUserId || !orgId) return
 setLoading(true)
 const client = new PostgRestClient(postgrestUrl, apiKey)

 Promise.all([
 analyticsApi.getUserPerformance(selectedUserId, orgId, fromDate, toDate).catch(() => null),
 analyticsApi.getUserInteractions(selectedUserId, orgId, days).catch(() => null),
 analyticsApi.getUserSentiment(selectedUserId, orgId, fromDate, toDate).catch(() => null),
 client.get<TaskRow>('tasks',
 new QueryBuilder().select('id,title,status,due_date,priority,assigned_to').eq('organization_id', orgId).eq('assigned_to', selectedUserId).build(),
 token,
 ).catch(() => [] as TaskRow[]),
 client.get<GoalRow>('goals',
 new QueryBuilder().select('id,title,status,target_date,created_by').eq('organization_id', orgId).eq('created_by', selectedUserId).build(),
 token,
 ).catch(() => [] as GoalRow[]),
 // Fetch positive comments on this user's tasks (kudos)
 client.get<CommentRow>('task_comments',
 new QueryBuilder().select('id,task_id,author_id,content,created_at').eq('organization_id', orgId).limit(5000).build(),
 token,
 ).catch(() => [] as CommentRow[]),
 ]).then(([perf, inter, sent, tasks, goals, allComments]) => {
 setPerformanceData(perf)
 setInteractionsData(inter)
 setSentimentData(sent)
 setTasksFromDb(tasks)
 setGoalsFromDb(goals)
 // Filter for kudos: comments by OTHER users on THIS user's tasks containing positive keywords
 const userTaskIds = new Set(tasks.map((t) => t.id))
 const kudos = (allComments as CommentRow[]).filter((c) =>
  userTaskIds.has(c.task_id) &&
  c.author_id !== selectedUserId &&
  /great|excellent|awesome|good job|well done|thank|kudos|amazing|outstanding|fantastic|nice work|bravo|impressive/i.test(c.content)
 )
 setKudosComments(kudos)
 }).finally(() => setLoading(false))

 // Fetch time entries from BPE timekeeping
 const { ragUrl } = detectApiUrls()
 const selectedUser = users.find((u) => u.id === selectedUserId)
 if (selectedUser) {
 // Find employee by name match
 fetch(`${ragUrl}/bpe/api/timekeeping/employees?organization_id=${orgId}`, {
  headers: { Authorization: `Bearer ${token}`, apikey: apiKey },
 })
 .then((r) => r.ok ? r.json() : { data: [] })
 .then((resp) => {
  const emps = resp.data || resp || []
  const emp = emps.find((e: { first_name: string; last_name: string }) =>
  e.first_name === selectedUser.first_name && e.last_name === selectedUser.last_name
  )
  if (emp) {
  fetch(`${ragUrl}/bpe/api/timekeeping/time-entries?organization_id=${orgId}&employee_id=${emp.id}`, {
   headers: { Authorization: `Bearer ${token}`, apikey: apiKey },
  })
  .then((r) => r.ok ? r.json() : { data: [] })
  .then((resp) => setTimeEntries(resp.data || resp || []))
  .catch(() => setTimeEntries([]))
  } else {
  setTimeEntries([])
  }
 })
 .catch(() => setTimeEntries([]))
 }
 }, [selectedUserId, orgId, days, fromDate, toDate, token, postgrestUrl, apiKey])

 const selectedUser = useMemo(() => users.find((u) => u.id === selectedUserId) || null, [users, selectedUserId])
 const managerName = useMemo(() => {
 if (!selectedUser?.manager_id) return 'None'
 const m = users.find((u) => u.id === selectedUser.manager_id)
 return m ? `${m.first_name} ${m.last_name}` : 'Unknown'
 }, [selectedUser, users])

 // Task metrics by status
 const taskMetrics = useMemo(() => {
 if (performanceData?.tasks && performanceData.tasks.total > 0) {
 const { tasks } = performanceData
 const bs = tasks.by_status || {}
 return {
 total: tasks.total,
 assigned: bs['pending'] || bs['Assigned'] || 0,
 inProgress: bs['in_progress'] || bs['In Progress'] || 0,
 completed: bs['completed'] || bs['Completed'] || tasks.completed || 0,
 overdue: bs['overdue'] || 0,
 }
 }
 const now = new Date()
 return {
 total: tasksFromDb.length,
 assigned: tasksFromDb.filter((t) => t.status === 'Assigned').length,
 inProgress: tasksFromDb.filter((t) => t.status === 'In Progress').length,
 completed: tasksFromDb.filter((t) => t.status === 'Completed').length,
 overdue: tasksFromDb.filter((t) => new Date(t.due_date) < now && t.status !== 'Completed').length,
 }
 }, [performanceData, tasksFromDb])

 // Task metrics by priority
 const taskByPriority = useMemo(() => {
 const counts: Record<string, number> = {}
 for (const t of tasksFromDb) {
 const p = t.priority || 'Unset'
 counts[p] = (counts[p] || 0) + 1
 }
 return counts
 }, [tasksFromDb])

 // Cross-tab: priority x status
 const taskCrossTab = useMemo(() => {
 const statuses = ['Assigned', 'In Progress', 'Completed']
 const priorities = ['Critical', 'High', 'Medium', 'Low']
 const grid: Record<string, Record<string, number>> = {}
 for (const p of priorities) {
 grid[p] = {}
 for (const s of statuses) grid[p][s] = 0
 }
 for (const t of tasksFromDb) {
 const p = priorities.includes(t.priority || '') ? t.priority! : null
 const s = statuses.includes(t.status) ? t.status : null
 if (p && s) grid[p][s]++
 }
 // Only include priorities that have at least 1 task
 const rows = priorities.filter((p) => statuses.some((s) => grid[p][s] > 0))
 return { statuses, rows, grid }
 }, [tasksFromDb])

 // Goal metrics
 const goalMetrics = useMemo(() => {
 if (performanceData?.goals && performanceData.goals.total > 0) {
 const { goals } = performanceData
 const bs = goals.by_status || {}
 return {
 total: goals.total,
 notStarted: bs['not_started'] || bs['active'] || 0,
 inProgress: bs['in_progress'] || 0,
 completed: bs['completed'] || goals.completed || 0,
 }
 }
 return {
 total: goalsFromDb.length,
 notStarted: goalsFromDb.filter((g) => g.status === 'not_started').length,
 inProgress: goalsFromDb.filter((g) => g.status === 'in_progress').length,
 completed: goalsFromDb.filter((g) => g.status === 'completed').length,
 }
 }, [performanceData, goalsFromDb])

 // Timekeeping summary
 const tkSummary = useMemo(() => {
 let shifts = 0, overtime = 0, vacation = 0, sick = 0
 for (const e of timeEntries) {
 if (e.pay_code === 'REG') shifts++
 else if (e.pay_code === 'OT') overtime += e.hours
 else if (e.pay_code === 'VACATION') vacation += e.hours
 else if (e.pay_code === 'SICK') sick += e.hours
 }
 return { shifts, overtime: Number(overtime.toFixed(1)), vacation: Number(vacation.toFixed(1)), sick: Number(sick.toFixed(1)) }
 }, [timeEntries])

 // Top collaborators (hidden but kept)
 const topCollaborators = useMemo(() => {
 if (interactionsData?.top_collaborators?.length) {
 return interactionsData.top_collaborators.map((c) => {
 const u = users.find((u) => u.id === c.user_id)
 return { name: u ? `${u.first_name} ${u.last_name}` : 'Unknown', count: c.interaction_count }
 })
 }
 return []
 }, [interactionsData, users])

 // Sentiment chart (hidden but kept)
 const sentimentChartData = useMemo(() => {
 if (!sentimentData?.time_series) return []
 return sentimentData.time_series.map((p) => ({
 date: p.date,
 sentiment: p.average_sentiment,
 count: p.interaction_count,
 }))
 }, [sentimentData])

 // Kudos with task titles
 const kudosWithContext = useMemo(() => {
 const taskMap = new Map(tasksFromDb.map((t) => [t.id, t.title]))
 return kudosComments.map((c) => ({
 ...c,
 authorName: userMap[c.author_id] || 'Unknown',
 taskTitle: taskMap.get(c.task_id) || 'Unknown task',
 })).slice(0, 10)
 }, [kudosComments, tasksFromDb, userMap])

 return (
 <div className="space-y-6">
 <div>
 <h1 className="text-2xl font-bold">Individual Performance</h1>
 <p className="text-muted-foreground text-sm">View performance metrics for team members</p>
 </div>

 {/* Controls */}
 <Card>
 <CardContent className="pt-6">
 <div className="flex flex-col gap-4 md:flex-row md:items-end">
  <div className="flex-1 space-y-2">
  <label className="text-sm font-medium">Select User</label>
  <Select value={selectedUserId} onValueChange={setSelectedUserId}>
   <SelectTrigger className="w-full md:w-80">
   <SelectValue placeholder="Select a user to view performance..." />
   </SelectTrigger>
   <SelectContent>
   {users.map((u) => (
   <SelectItem key={u.id} value={u.id}>
    {u.first_name} {u.last_name}
    {u.title && <span className="text-muted-foreground ml-2">({u.title})</span>}
   </SelectItem>
   ))}
   </SelectContent>
  </Select>
  </div>
  <div className="flex-1 space-y-2">
  <label className="text-sm font-medium">Time Period: <span className="text-primary">{days} days</span></label>
  <Slider value={[days]} onValueChange={(v) => setDays(v[0])} min={7} max={90} step={7} className="w-full md:w-64" />
  </div>
 </div>
 </CardContent>
 </Card>

 {!selectedUserId ? (
 <Card>
 <CardContent className="py-12 text-center text-muted-foreground">
  <User className="h-12 w-12 mx-auto mb-4 opacity-50" />
  <p>Select a user above to view their performance metrics</p>
 </CardContent>
 </Card>
 ) : (
 <div className="space-y-6">
 {/* Timekeeping Stats */}
 {!loading && (
 <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
  <Card>
  <CardContent className="pt-4 pb-4">
   <div className="flex items-center gap-2 mb-1"><CalendarDays className="h-4 w-4 text-muted-foreground" /><p className="text-xs text-muted-foreground">Shifts Worked</p></div>
   <p className="text-2xl font-bold">{tkSummary.shifts}</p>
  </CardContent>
  </Card>
  <Card>
  <CardContent className="pt-4 pb-4">
   <div className="flex items-center gap-2 mb-1"><Flame className="h-4 w-4 text-orange-500" /><p className="text-xs text-muted-foreground">Overtime Hours</p></div>
   <p className="text-2xl font-bold">{tkSummary.overtime}</p>
  </CardContent>
  </Card>
  <Card>
  <CardContent className="pt-4 pb-4">
   <div className="flex items-center gap-2 mb-1"><CalendarDays className="h-4 w-4 text-blue-500" /><p className="text-xs text-muted-foreground">Vacation (hrs)</p></div>
   <p className="text-2xl font-bold">{tkSummary.vacation}</p>
  </CardContent>
  </Card>
  <Card>
  <CardContent className="pt-4 pb-4">
   <div className="flex items-center gap-2 mb-1"><AlertTriangle className="h-4 w-4 text-amber-500" /><p className="text-xs text-muted-foreground">Sick Days (hrs)</p></div>
   <p className="text-2xl font-bold">{tkSummary.sick}</p>
  </CardContent>
  </Card>
 </div>
 )}

 {/* User Profile Card */}
 <Card>
 <CardContent className="pt-6">
  {loading ? (
  <div className="flex items-center gap-4">
   <Skeleton className="h-14 w-14 rounded-full" />
   <div className="space-y-2 flex-1"><Skeleton className="h-5 w-48" /><Skeleton className="h-4 w-32" /></div>
  </div>
  ) : selectedUser ? (
  <div className="flex flex-col md:flex-row md:items-center gap-4">
   <div className="flex items-center gap-4 flex-1">
   <div className="h-14 w-14 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-lg font-bold border-2 border-primary">
    {selectedUser.first_name[0]}{selectedUser.last_name[0]}
   </div>
   <div>
    <h3 className="text-lg font-semibold">{selectedUser.first_name} {selectedUser.last_name}</h3>
    <div className="flex items-center gap-4 text-sm text-muted-foreground">
    {selectedUser.title && <span>{selectedUser.title}</span>}
    <span>Manager: {managerName}</span>
    </div>
   </div>
   </div>
   <div className="flex flex-col gap-2 md:items-end">
   {goalMetrics.total > 0 && (
    <div className="flex items-center gap-2">
    <Target className="h-4 w-4 text-muted-foreground" />
    <span className="text-sm font-medium w-12">Goals</span>
    <div className="flex items-center gap-1.5">
     <Badge variant="secondary" className="text-xs px-2 py-0.5">{goalMetrics.notStarted} Not Started</Badge>
     <Badge className="bg-primary/10 text-primary text-xs px-2 py-0.5">{goalMetrics.inProgress} In Progress</Badge>
     <Badge className="bg-emerald-500/10 text-emerald-600 text-xs px-2 py-0.5">{goalMetrics.completed} Done</Badge>
    </div>
    </div>
   )}
   <div className="flex items-center gap-2">
    <CheckSquare className="h-4 w-4 text-muted-foreground" />
    <span className="text-sm font-medium">Tasks: {taskMetrics.total}</span>
    {taskMetrics.overdue > 0 && <Badge variant="destructive" className="text-xs px-2 py-0.5">{taskMetrics.overdue} Overdue</Badge>}
   </div>
   </div>
  </div>
  ) : <p className="text-muted-foreground">User not found</p>}
 </CardContent>
 </Card>

 {/* Tasks by Priority & Status */}
 {!loading && tasksFromDb.length > 0 && taskCrossTab.rows.length > 0 && (
 <Card>
  <CardHeader className="pb-2">
  <CardTitle className="flex items-center gap-2 text-base"><CheckSquare className="h-4 w-4" />Tasks by Priority &amp; Status</CardTitle>
  </CardHeader>
  <CardContent className="pb-4 overflow-x-auto">
  <table className="w-full text-sm">
   <thead>
   <tr className="border-b bg-muted/50">
    <th className="px-3 py-2 text-left text-xs font-semibold text-muted-foreground uppercase">Priority</th>
    {taskCrossTab.statuses.map((s) => (
    <th key={s} className="px-3 py-2 text-center text-xs font-semibold text-muted-foreground uppercase">{s}</th>
    ))}
    <th className="px-3 py-2 text-center text-xs font-semibold text-muted-foreground uppercase">Total</th>
   </tr>
   </thead>
   <tbody>
   {taskCrossTab.rows.map((p) => {
    const rowTotal = taskCrossTab.statuses.reduce((sum, s) => sum + taskCrossTab.grid[p][s], 0)
    return (
    <tr key={p} className="border-b last:border-0 hover:bg-muted/30">
     <td className="px-3 py-2">
     <Badge className={`text-xs ${PRIORITY_COLORS[p] || 'bg-gray-100 text-gray-600'}`}>{p}</Badge>
     </td>
     {taskCrossTab.statuses.map((s) => (
     <td key={s} className="px-3 py-2 text-center font-medium">
      {taskCrossTab.grid[p][s] || <span className="text-muted-foreground">-</span>}
     </td>
     ))}
     <td className="px-3 py-2 text-center font-bold">{rowTotal}</td>
    </tr>
    )
   })}
   <tr className="bg-muted/50 font-bold">
    <td className="px-3 py-2">Total</td>
    {taskCrossTab.statuses.map((s) => (
    <td key={s} className="px-3 py-2 text-center">
     {taskCrossTab.rows.reduce((sum, p) => sum + taskCrossTab.grid[p][s], 0)}
    </td>
    ))}
    <td className="px-3 py-2 text-center">{tasksFromDb.length}</td>
   </tr>
   </tbody>
  </table>
  </CardContent>
 </Card>
 )}

 {/* Kudos Received */}
 <Card>
 <CardHeader className="pb-2">
  <CardTitle className="flex items-center gap-2 text-base"><ThumbsUp className="h-4 w-4" />Kudos Received</CardTitle>
  <CardDescription className="text-xs">Positive recognition from teammates</CardDescription>
 </CardHeader>
 <CardContent className="pb-4">
  {loading ? <Skeleton className="h-[180px]" /> : kudosWithContext.length > 0 ? (
  <TooltipProvider>
   <div className="space-y-2">
   {kudosWithContext.map((k) => (
    <UITooltip key={k.id}>
    <TooltipTrigger asChild>
     <div className="flex items-center justify-between p-2 rounded-lg bg-emerald-50 border border-emerald-100 cursor-pointer hover:bg-emerald-100 transition-colors">
     <div className="flex-1 min-w-0">
      <p className="text-sm font-medium text-emerald-800">{k.authorName}</p>
      <p className="text-xs text-emerald-600 truncate">on: {k.taskTitle}</p>
     </div>
     <ThumbsUp className="h-4 w-4 text-emerald-500 shrink-0 ml-2" />
     </div>
    </TooltipTrigger>
    <TooltipContent className="max-w-sm">
     <p className="text-sm">{k.content}</p>
     <p className="text-xs text-muted-foreground mt-1">{new Date(k.created_at).toLocaleDateString()}</p>
    </TooltipContent>
    </UITooltip>
   ))}
   </div>
  </TooltipProvider>
  ) : (
  <div className="h-[120px] flex items-center justify-center text-muted-foreground text-sm">
   No kudos received yet
  </div>
  )}
 </CardContent>
 </Card>

 {/* Hidden: Interactions */}
 {false && <Card>
 <CardHeader className="pb-2">
  <CardTitle className="flex items-center gap-2 text-base"><Users className="h-4 w-4" />Interactions</CardTitle>
  <CardDescription className="text-xs">Top collaborators in the last {days} days</CardDescription>
 </CardHeader>
 <CardContent className="pb-4">
  {loading ? <Skeleton className="h-[180px]" /> : topCollaborators.length > 0 ? (
  <div className="space-y-2">
   {topCollaborators.slice(0, 8).map((c, i) => (
   <div key={i} className="flex items-center justify-between p-2 rounded-lg bg-muted/30">
    <span className="text-sm font-medium">{c.name}</span>
    <Badge variant="secondary">{c.count}</Badge>
   </div>
   ))}
  </div>
  ) : (
  <div className="h-[180px] flex items-center justify-center text-muted-foreground text-sm">
   {interactionsData ? 'No interaction data available' : 'Analytics API not available — interaction data requires the analytics endpoints.'}
  </div>
  )}
 </CardContent>
 </Card>}

 {/* Hidden: Sentiment Chart */}
 {false && <Card>
 <CardHeader className="pb-2">
  <CardTitle className="flex items-center gap-2 text-base"><MessageSquare className="h-4 w-4" />Sentiment Trend</CardTitle>
  <CardDescription className="text-xs">Communication sentiment over time</CardDescription>
 </CardHeader>
 <CardContent className="pb-4">
  {loading ? <Skeleton className="h-[180px]" /> : sentimentChartData.length > 0 ? (
  <ResponsiveContainer width="100%" height={180}>
   <LineChart data={sentimentChartData} margin={{ top: 5, right: 10, bottom: 15, left: -20 }}>
   <CartesianGrid strokeDasharray="3 3" />
   <XAxis dataKey="date" tick={{ fontSize: 10 }}
    tickFormatter={(v) => { try { return new Date(v).toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) } catch { return v } }} />
   <YAxis domain={[-1, 1]} tick={{ fontSize: 10 }} />
   <Tooltip formatter={(v) => [Number(v).toFixed(2), 'Sentiment']} />
   <ReferenceLine y={0} strokeDasharray="3 3" />
   <Line type="monotone" dataKey="sentiment" stroke="hsl(221, 83%, 53%)" strokeWidth={2} dot={{ fill: 'hsl(221, 83%, 53%)', r: 2 }} />
   </LineChart>
  </ResponsiveContainer>
  ) : (
  <div className="h-[180px] flex items-center justify-center text-muted-foreground text-sm">
   No sentiment data available
  </div>
  )}
 </CardContent>
 </Card>}
 </div>
 )}
 </div>
 )
}
