import { useState, useEffect, useCallback, useMemo } from 'react'
import { Link } from 'react-router-dom'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Loader2, Users } from 'lucide-react'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import type { Task } from '@/models/task'

export function TeamWorkloadPage() {
 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [tasks, setTasks] = useState<Task[]>([])
 const [nameMap, setNameMap] = useState<Map<string, string>>(new Map())
 const [isLoading, setIsLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 const fetchData = useCallback(async () => {
 if (!orgId) { setIsLoading(false); return }
 setIsLoading(true)
 setError(null)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const [taskData, userData] = await Promise.all([
 client.get<Task>(
 'tasks',
 new QueryBuilder()
 .select('id,organization_id,title,status,priority,assigned_to')
 .eq('organization_id', orgId)
 .limit(500)
 .build(),
 token,
 ),
 client.get<{ id: string; first_name: string; last_name: string }>(
 'users',
 new QueryBuilder()
 .select('id,first_name,last_name')
 .eq('organization_id', orgId)
 .build(),
 token,
 ),
 ])
 setTasks(taskData)
 setNameMap(new Map(userData.map((u) => [u.id, `${u.first_name} ${u.last_name}`.trim()])))
 } catch (err) {
 setError(err instanceof Error ? err.message : String(err))
 } finally {
 setIsLoading(false)
 }
 }, [orgId, token, postgrestUrl, apiKey])

 useEffect(() => {
 let cancelled = false
 fetchData().finally(() => { if (cancelled) return })
 return () => { cancelled = true }
 }, [fetchData])

 const workload = useMemo(() => {
 const map = new Map<string, { total: number; completed: number; inProgress: number }>()
 for (const t of tasks) {
 const key = t.assigned_to ?? 'Unassigned'
 const entry = map.get(key) ?? { total: 0, completed: 0, inProgress: 0 }
 entry.total++
 if (t.status === 'Completed') entry.completed++
 if (t.status === 'In Progress') entry.inProgress++
 map.set(key, entry)
 }
 return [...map.entries()].sort((a, b) => b[1].total - a[1].total)
 }, [tasks])

 return (
 <div className="space-y-4">
 <div className="flex items-center gap-2">
 <Users className="w-5 h-5 text-muted-foreground" />
 <h1 className="text-2xl font-bold text-foreground">Team Workload</h1>
 </div>

 {error && (
 <div className="p-3 text-sm text-destructive bg-destructive/10 border border-destructive/20 rounded-lg">
 {error}
 </div>
 )}

 {isLoading ? (
 <div className="flex items-center gap-2 text-sm text-muted-foreground">
 <Loader2 className="w-4 h-4 animate-spin" />
 Loading workload...
 </div>
 ) : (
 <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
 {workload.map(([userId, stats]) => {
 const pct = stats.total > 0 ? Math.round((stats.completed / stats.total) * 100) : 0
 const isUnassigned = userId === 'Unassigned'
 const displayName = isUnassigned ? 'Unassigned' : (nameMap.get(userId) || userId)
 const Wrapper = isUnassigned ? 'div' : Link
 const wrapperProps = isUnassigned
 ? { className: 'block' }
 : { to: `/team/${userId}`, className: 'block' }
 return (
 <Wrapper key={userId} {...(wrapperProps as any)}>
 <Card className={`transition-colors ${isUnassigned ? '' : 'hover:bg-muted/50 cursor-pointer'}`}>
 <CardContent className="p-5">
 <p className="text-sm font-medium text-foreground truncate">{displayName}</p>
 <div className="mt-3 space-y-2">
 <div className="flex justify-between text-xs text-muted-foreground">
 <span>Total tasks</span>
 <span className="font-semibold text-foreground">{stats.total}</span>
 </div>
 <div className="flex justify-between text-xs text-muted-foreground">
 <span>In Progress</span>
 <Badge variant="secondary" className="text-xs px-1.5 py-0">{stats.inProgress}</Badge>
 </div>
 <div className="flex justify-between text-xs text-muted-foreground">
 <span>Completed</span>
 <Badge variant="outline" className="text-xs px-1.5 py-0 text-green-600 border-green-300">
 {stats.completed}
 </Badge>
 </div>
 <div className="pt-1">
 <Progress value={pct} className="h-1.5" />
 <p className="text-xs text-muted-foreground mt-1 text-right">{pct}% complete</p>
 </div>
 </div>
 </CardContent>
 </Card>
 </Wrapper>
 )
 })}
 {workload.length === 0 && (
 <p className="text-sm text-muted-foreground col-span-full">No workload data.</p>
 )}
 </div>
 )}
 </div>
 )
}
