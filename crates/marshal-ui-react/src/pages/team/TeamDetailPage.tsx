import { useState, useEffect, useCallback } from 'react'
import { Link, useParams } from 'react-router-dom'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Loader2, ArrowLeft, Mail, Phone, Briefcase, BarChart3 } from 'lucide-react'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { detectApiUrls } from '@/lib/config'
import type { User } from '@/models/user'
import type { Task } from '@/models/task'

export function TeamDetailPage() {
 const { id } = useParams<{ id: string }>()
 const token = useAuthStore((s) => s.token)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [user, setUser] = useState<User | null>(null)
 const [tasks, setTasks] = useState<Task[]>([])
 const [isLoading, setIsLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 const fetchData = useCallback(async () => {
 if (!id) return
 setIsLoading(true)
 setError(null)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)

 const userQs = new QueryBuilder().select('*').eq('id', id).build()
 const taskQs = new QueryBuilder()
 .select('id,title,status,priority,due_date')
 .eq('assigned_to', id)
 .order('created_at', false)
 .limit(50)
 .build()

 const [userData, taskData] = await Promise.all([
 client.getOne<User>('users', userQs, token),
 client.get<Task>('tasks', taskQs, token),
 ])

 setUser(userData)
 setTasks(taskData)
 } catch (err) {
 setError(err instanceof Error ? err.message : String(err))
 } finally {
 setIsLoading(false)
 }
 }, [id, token, postgrestUrl, apiKey])

 useEffect(() => {
 fetchData()
 }, [fetchData])

 const name = user
 ? [user.first_name, user.last_name].filter(Boolean).join(' ') || 'Unknown'
 : 'Unknown'

 const initial = name.charAt(0).toUpperCase()

 const statusColor = (s: string) => {
 switch (s) {
 case 'Completed': return 'bg-emerald-100 text-emerald-700'
 case 'In Progress': return 'bg-blue-100 text-blue-700'
 case 'Assigned': return 'bg-amber-100 text-amber-700'
 default: return 'bg-muted text-muted-foreground'
 }
 }

 return (
 <div className="space-y-6">
 <Button variant="ghost" size="sm" asChild>
 <Link to="/team-workload" className="inline-flex items-center gap-1 text-sm text-muted-foreground">
 <ArrowLeft className="w-4 h-4" /> Back to Team
 </Link>
 </Button>

 {error && (
 <div className="p-3 text-sm text-destructive bg-destructive/10 border border-destructive/20 rounded-lg">
 {error}
 </div>
 )}

 {isLoading && (
 <div className="flex items-center gap-2 text-sm text-muted-foreground">
 <Loader2 className="w-4 h-4 animate-spin" />
 Loading team member...
 </div>
 )}

 {user && (
 <>
 {/* Profile header */}
 <div className="flex items-center gap-4">
 <div className="w-16 h-16 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-xl font-bold">
 {initial}
 </div>
 <div>
 <h1 className="text-2xl font-bold text-foreground">{name}</h1>
 <p className="text-muted-foreground">{user.title ?? 'No title'}</p>
 </div>
 </div>

 {/* Info cards */}
 <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
 <Card>
 <CardContent className="p-4 flex flex-col gap-1">
 <span className="flex items-center gap-1 text-xs text-muted-foreground uppercase tracking-wider">
 <Mail className="w-3 h-3" /> Email
 </span>
 <span className="text-sm font-medium text-foreground truncate">{user.email ?? '--'}</span>
 </CardContent>
 </Card>
 <Card>
 <CardContent className="p-4 flex flex-col gap-1">
 <span className="flex items-center gap-1 text-xs text-muted-foreground uppercase tracking-wider">
 <BarChart3 className="w-3 h-3" /> Level
 </span>
 <span className="text-sm font-medium text-foreground">{user.level ?? '--'}</span>
 </CardContent>
 </Card>
 <Card>
 <CardContent className="p-4 flex flex-col gap-1">
 <span className="flex items-center gap-1 text-xs text-muted-foreground uppercase tracking-wider">
 <Phone className="w-3 h-3" /> Phone
 </span>
 <span className="text-sm font-medium text-foreground">{user.mobile_phone ?? '--'}</span>
 </CardContent>
 </Card>
 <Card>
 <CardContent className="p-4 flex flex-col gap-1">
 <span className="flex items-center gap-1 text-xs text-muted-foreground uppercase tracking-wider">
 <Briefcase className="w-3 h-3" /> Username
 </span>
 <span className="text-sm font-medium text-foreground">{user.username ?? '--'}</span>
 </CardContent>
 </Card>
 </div>

 {/* Assigned tasks */}
 <Card>
 <CardHeader className="pb-3">
 <CardTitle className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
 Assigned Tasks ({tasks.length})
 </CardTitle>
 </CardHeader>
 <CardContent className="p-0">
 {tasks.length === 0 ? (
 <p className="px-5 py-8 text-center text-sm text-muted-foreground">No tasks assigned.</p>
 ) : (
 <div className="divide-y">
 {tasks.map((t) => (
 <Link
 key={t.id}
 to={`/tasks/${t.id}`}
 className="flex items-center justify-between px-5 py-3 hover:bg-muted/50 transition-colors"
 >
 <div className="min-w-0 flex-1">
 <p className="text-sm font-medium text-foreground truncate">{t.title}</p>
 <div className="flex items-center gap-2 mt-0.5">
 {t.priority && <span className="text-xs text-muted-foreground">{t.priority}</span>}
 {t.due_date && <span className="text-xs text-muted-foreground">Due {t.due_date.slice(0, 10)}</span>}
 </div>
 </div>
 <span className={`ml-3 px-2 py-0.5 rounded text-xs font-medium flex-shrink-0 ${statusColor(t.status)}`}>
 {t.status}
 </span>
 </Link>
 ))}
 </div>
 )}
 </CardContent>
 </Card>
 </>
 )}
 </div>
 )
}
