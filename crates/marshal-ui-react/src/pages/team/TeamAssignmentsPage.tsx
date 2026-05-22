import { useState, useEffect, useCallback } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Card, CardContent } from '@/components/ui/card'
import { Loader2, ClipboardList } from 'lucide-react'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { StatusBadge } from '@/components/ui/StatusBadge'
import { MonthFilter } from '@/components/MonthFilter'

interface TaskRow {
 id: string
 title: string
 status: string
 priority: string
 due_date: string
 assigned_to: string | null
}

export function TeamAssignmentsPage() {
 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')
 const { postgrestUrl, apiKey } = detectApiUrls()
 const navigate = useNavigate()
 const [searchParams] = useSearchParams()

 const initialMonth = searchParams.get('month')
  ? new Date(searchParams.get('month')! + '-01')
  : new Date()
 const [selectedMonth, setSelectedMonth] = useState(initialMonth)

 const [userMap, setUserMap] = useState<Record<string, string>>({})
 const [isLoading, setIsLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Fetch all tasks once, filter by month client-side
 const [allTasks, setAllTasks] = useState<TaskRow[]>([])

 const fetchData = useCallback(async () => {
 if (!orgId) return
 setIsLoading(true)
 setError(null)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)

 const [users, data] = await Promise.all([
 client.get<{ id: string; first_name: string; last_name: string }>(
  'users',
  new QueryBuilder().select('id,first_name,last_name').eq('organization_id', orgId).limit(200).build(),
  token,
 ),
 client.get<TaskRow>(
  'tasks',
  new QueryBuilder()
  .select('id,title,status,priority,due_date,assigned_to')
  .eq('organization_id', orgId)
  .order('due_date', true)
  .limit(2500)
  .build(),
  token,
 ),
 ])

 const map: Record<string, string> = {}
 for (const u of users) map[u.id] = `${u.first_name} ${u.last_name}`
 setUserMap(map)
 setAllTasks(data)
 } catch (err) {
 setError(err instanceof Error ? err.message : String(err))
 } finally {
 setIsLoading(false)
 }
 }, [orgId, token, postgrestUrl, apiKey])

 // Filter by selected month client-side
 const filteredTasks = allTasks.filter((t) => {
 if (!t.due_date) return false
 const d = new Date(t.due_date + 'T00:00:00')
 return d.getFullYear() === selectedMonth.getFullYear() && d.getMonth() === selectedMonth.getMonth()
 })

 useEffect(() => {
 fetchData()
 }, [fetchData])

 return (
 <div className="space-y-4">
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-2">
  <ClipboardList className="w-5 h-5 text-muted-foreground" />
  <h1 className="text-2xl font-bold text-foreground">Team Assignments</h1>
 </div>
 <MonthFilter selectedMonth={selectedMonth} onMonthChange={setSelectedMonth} compact />
 </div>

 {error && (
 <div className="p-3 text-sm text-destructive bg-destructive/10 border border-destructive/20 rounded-lg">
  {error}
 </div>
 )}

 {isLoading ? (
 <div className="flex items-center gap-2 text-sm text-muted-foreground">
  <Loader2 className="w-4 h-4 animate-spin" />
  Loading assignments...
 </div>
 ) : (
 <Card>
  <CardContent className="p-0 overflow-x-auto">
  <table className="w-full text-left min-w-[640px]">
   <thead>
   <tr className="border-b bg-muted/50">
    <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Task</th>
    <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Status</th>
    <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Priority</th>
    <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Assigned To</th>
    <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Due Date</th>
   </tr>
   </thead>
   <tbody>
   {filteredTasks.map((t) => (
    <tr key={t.id} className="hover:bg-muted/50 border-b last:border-0 cursor-pointer" onClick={() => navigate(`/tasks/${t.id}`)}>
    <td className="px-4 py-3 text-sm font-medium text-primary hover:underline">{t.title}</td>
    <td className="px-4 py-3 text-sm">
     <StatusBadge status={t.status ?? 'Unknown'} />
    </td>
    <td className="px-4 py-3 text-sm text-muted-foreground">{t.priority}</td>
    <td className="px-4 py-3 text-sm text-muted-foreground">{(t.assigned_to && userMap[t.assigned_to]) || 'Unassigned'}</td>
    <td className="px-4 py-3 text-sm text-muted-foreground">{t.due_date}</td>
    </tr>
   ))}
   {filteredTasks.length === 0 && (
    <tr>
    <td colSpan={5} className="px-4 py-8 text-center text-sm text-muted-foreground">
     No assignments found for this month.
    </td>
    </tr>
   )}
   </tbody>
  </table>
  </CardContent>
 </Card>
 )}
 </div>
 )
}
