import { useState, useEffect, useCallback, useMemo } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { StatusBadge, PriorityBadge } from '@/components/ui/StatusBadge'
import { TaskDialog } from '@/components/TaskDialog'
import { Loader2, Plus, ListTodo, Target } from 'lucide-react'
import type { Task } from '@/models/task'
import type { Goal } from '@/models/goal'

interface UserOption { id: string; first_name: string; last_name: string }

export function TaskListPage() {
 const [searchParams] = useSearchParams()
 const [search, setSearch] = useState('')
 const [statusFilter, setStatusFilter] = useState('')
 const [priorityFilter, setPriorityFilter] = useState('')
 const [assigneeFilter, setAssigneeFilter] = useState(searchParams.get('assignee') || '')
 const [tasks, setTasks] = useState<Task[]>([])
 const [goalMap, setGoalMap] = useState<Map<string, string>>(new Map())
 const [userMap, setUserMap] = useState<Map<string, string>>(new Map())
 const [users, setUsers] = useState<UserOption[]>([])
 const [isLoading, setIsLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [dialogOpen, setDialogOpen] = useState(false)

 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const fetchTasks = useCallback(async () => {
 if (!currentOrg?.id) return
 setIsLoading(true)
 setError(null)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const [data, goalsData, usersData] = await Promise.all([
 client.get<Task>(
 'tasks',
 new QueryBuilder()
 .select('id,organization_id,title,status,priority,assigned_to,due_date,task_number,goal_id,created_at')
 .eq('organization_id', currentOrg.id)
 .order('created_at', false)
 .limit(200)
 .build(),
 token,
 ),
 client.get<Goal>(
 'goals',
 new QueryBuilder()
 .select('id,title')
 .eq('organization_id', currentOrg.id)
 .build(),
 token,
 ),
 client.get<UserOption>(
 'users',
 new QueryBuilder()
 .select('id,first_name,last_name')
 .eq('organization_id', currentOrg.id)
 .order('first_name', true)
 .limit(200)
 .build(),
 token,
 ),
 ])
 setTasks(data)
 const gm = new Map<string, string>()
 for (const g of goalsData) gm.set(g.id, g.title)
 setGoalMap(gm)
 const um = new Map<string, string>()
 for (const u of usersData) um.set(u.id, `${u.first_name} ${u.last_name}`)
 setUserMap(um)
 setUsers(usersData)
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to load tasks'
 setError(msg)
 } finally {
 setIsLoading(false)
 }
 }, [currentOrg?.id, postgrestUrl, apiKey, token])

 useEffect(() => {
 fetchTasks()
 }, [fetchTasks])

 const filtered = useMemo(() => {
 const q = search.toLowerCase()
 return tasks.filter((t) => {
 if (statusFilter && t.status !== statusFilter) return false
 if (priorityFilter && (t.priority ?? '') !== priorityFilter) return false
 if (assigneeFilter && (t.assigned_to ?? '') !== assigneeFilter) return false
 if (q && !t.title.toLowerCase().includes(q)) return false
 return true
 })
 }, [tasks, search, statusFilter, priorityFilter, assigneeFilter])

 return (
 <div className="space-y-4">
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <ListTodo className="h-6 w-6 text-primary" />
 <h1 className="text-2xl font-bold text-foreground">Tasks</h1>
 <Badge variant="secondary">
 {filtered.length === tasks.length
 ? `${tasks.length}`
 : `${filtered.length} / ${tasks.length}`}
 </Badge>
 </div>
 <Button onClick={() => setDialogOpen(true)}>
 <Plus className="mr-2 h-4 w-4" />
 Add Task
 </Button>
 </div>

 <div className="flex flex-wrap items-center gap-3">
 <Input
 type="text"
 placeholder="Search by title..."
 className="w-64"
 value={search}
 onChange={(e) => setSearch(e.target.value)}
 />
 <Select value={statusFilter || 'all'} onValueChange={(v) => setStatusFilter(v === 'all' ? '' : v)}>
 <SelectTrigger className="w-[160px]">
 <SelectValue placeholder="All Statuses" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="all">All Statuses</SelectItem>
 <SelectItem value="Assigned">Assigned</SelectItem>
 <SelectItem value="In Progress">In Progress</SelectItem>
 <SelectItem value="Completed">Completed</SelectItem>
 <SelectItem value="Blocked">Blocked</SelectItem>
 <SelectItem value="On Hold">On Hold</SelectItem>
 <SelectItem value="Cancelled">Cancelled</SelectItem>
 </SelectContent>
 </Select>
 <Select value={priorityFilter || 'all'} onValueChange={(v) => setPriorityFilter(v === 'all' ? '' : v)}>
 <SelectTrigger className="w-[160px]">
 <SelectValue placeholder="All Priorities" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="all">All Priorities</SelectItem>
 <SelectItem value="Critical">Critical</SelectItem>
 <SelectItem value="High">High</SelectItem>
 <SelectItem value="Medium">Medium</SelectItem>
 <SelectItem value="Low">Low</SelectItem>
 </SelectContent>
 </Select>
 <Select value={assigneeFilter || 'all'} onValueChange={(v) => setAssigneeFilter(v === 'all' ? '' : v)}>
 <SelectTrigger className="w-[200px]">
 <SelectValue placeholder="All Assignees" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="all">All Assignees</SelectItem>
 {users.map((u) => (
  <SelectItem key={u.id} value={u.id}>{u.first_name} {u.last_name}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>

 {error && (
 <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
 {error}
 </div>
 )}

 {isLoading ? (
 <div className="flex items-center gap-2 py-8 justify-center text-muted-foreground">
 <Loader2 className="h-5 w-5 animate-spin" />
 <span className="text-sm">Loading tasks...</span>
 </div>
 ) : (
 <Card>
 <CardContent className="p-0 overflow-x-auto">
 <table className="w-full text-left min-w-[640px]">
 <thead>
 <tr className="border-b bg-muted/50">
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Title</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Goal</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Assigned To</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Status</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Priority</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Due Date</th>
 </tr>
 </thead>
 <tbody>
 {filtered.map((t) => (
 <tr key={t.id} className="hover:bg-muted/50 border-b last:border-0">
 <td className="px-4 py-3 text-sm">
 <Link to={`/tasks/${t.id}`} className="text-primary hover:underline font-medium">
 {t.title}
 </Link>
 </td>
 <td className="px-4 py-3 text-sm">
 {t.goal_id && goalMap.has(t.goal_id) ? (
 <Link to={`/goals/${t.goal_id}`} className="text-xs text-primary hover:underline flex items-center gap-1">
 <Target className="h-3 w-3" />
 {goalMap.get(t.goal_id)}
 </Link>
 ) : (
 <span className="text-xs text-muted-foreground">--</span>
 )}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">{(t.assigned_to && userMap.get(t.assigned_to)) || 'Unassigned'}</td>
 <td className="px-4 py-3 text-sm"><StatusBadge status={t.status} /></td>
 <td className="px-4 py-3 text-sm"><PriorityBadge priority={t.priority} /></td>
 <td className="px-4 py-3 text-sm text-muted-foreground">{t.due_date ?? '--'}</td>
 </tr>
 ))}
 {filtered.length === 0 && (
 <tr>
 <td colSpan={6} className="px-4 py-8 text-center text-sm text-muted-foreground">No tasks found.</td>
 </tr>
 )}
 </tbody>
 </table>
 </CardContent>
 </Card>
 )}

 <TaskDialog open={dialogOpen} onOpenChange={setDialogOpen} onTaskCreated={fetchTasks} />
 </div>
 )
}
