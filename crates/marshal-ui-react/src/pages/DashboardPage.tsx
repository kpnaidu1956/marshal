import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { MonthFilter } from '@/components/MonthFilter'
import { format, startOfMonth, endOfMonth } from 'date-fns'
import { Loader2 } from 'lucide-react'
import { PieChart, Pie, Cell, ResponsiveContainer, Legend } from 'recharts'

interface TaskStats {
 assigned: number
 inProgress: number
 overdue: number
 completed: number
}

interface UserTask {
 id: string
 title: string
 due_date: string
 priority: string
 status: string
}

interface SpecialEvent {
 id: string
 title: string
 event_date: string
}

interface UserMessage {
 id: string
 senderName: string
 preview: string
 createdAt: string
 taskId: string | null
 isRead: boolean
}

const PRIORITY_COLORS: Record<string, string> = {
 high: 'bg-red-100 text-red-700',
 medium: 'bg-amber-100 text-amber-700',
 low: 'bg-emerald-100 text-emerald-700',
}

const PIE_COLORS = [
 'hsl(220, 13%, 40%)', // Assigned
 'hsl(45, 100%, 50%)', // In-progress
 'hsl(0, 72%, 51%)', // Overdue
 'hsl(120, 60%, 50%)', // Completed
]

function formatMessageTime(dateString: string) {
 const date = new Date(dateString)
 const now = new Date()
 const diffHours = (now.getTime() - date.getTime()) / (1000 * 60 * 60)
 if (diffHours < 24) return format(date, 'h:mm a')
 if (diffHours < 48) return 'Yesterday'
 return format(date, 'MMM d')
}

export function DashboardPage() {
 const navigate = useNavigate()
 const user = useAuthStore((s) => s.user)
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [loading, setLoading] = useState(true)
 const [selectedMonth, setSelectedMonth] = useState(new Date())
 const [taskStats, setTaskStats] = useState<TaskStats>({ assigned: 0, inProgress: 0, overdue: 0, completed: 0 })
 const [userTasks, setUserTasks] = useState<UserTask[]>([])
 const [specialEvents, setSpecialEvents] = useState<SpecialEvent[]>([])
 const [userMessages, setUserMessages] = useState<UserMessage[]>([])

 const userId = user?.id ?? ''
 const orgId = currentOrg?.id ?? ''

 useEffect(() => {
 if (!orgId) {
 setLoading(false)
 return
 }
 let cancelled = false
 setLoading(true)

 const client = new PostgRestClient(postgrestUrl, apiKey)
 const monthStart = format(startOfMonth(selectedMonth), 'yyyy-MM-dd')
 const monthEnd = format(endOfMonth(selectedMonth), 'yyyy-MM-dd')
 const today = new Date().toISOString().split('T')[0]

 const loadStats = async () => {
 const tasks = await client.get<{ status: string; due_date: string; assigned_to: string | null }>(
 'tasks',
 new QueryBuilder().select('status,due_date,assigned_to').eq('organization_id', orgId).build(),
 token,
 ).catch(() => [] as { status: string; due_date: string; assigned_to: string | null }[])

 if (cancelled) return

 const filtered = tasks.filter((t) => {
 if (!t.assigned_to) return false
 const d = (t.due_date || '').split('T')[0]
 const s = (t.status || '').trim().toLowerCase()
 const done = s === 'completed'
 const inMonth = d >= monthStart && d <= monthEnd
 const overduePrev = d < monthStart && d < today && !done
 if (d < monthStart && done) return false
 return inMonth || overduePrev
 })

 const stats: TaskStats = { assigned: 0, inProgress: 0, overdue: 0, completed: 0 }
 filtered.forEach((t) => {
 const d = (t.due_date || '').split('T')[0]
 const s = (t.status || '').trim().toLowerCase()
 const done = s === 'completed'
 if (d < today && !done) stats.overdue++
 else if (done) stats.completed++
 else if (s === 'in-progress' || s === 'in progress') stats.inProgress++
 else if (s === 'assigned') stats.assigned++
 })
 if (!cancelled) setTaskStats(stats)
 }

 const loadUserTasks = async () => {
 if (!userId) return
 const tasks = await client.get<UserTask>(
 'tasks',
 new QueryBuilder()
 .select('id,title,due_date,priority,status')
 .eq('assigned_to', userId)
 .eq('organization_id', orgId)
 .gte('due_date', monthStart)
 .lte('due_date', monthEnd)
 .order('due_date', true)
 .build(),
 token,
 ).catch(() => [] as UserTask[])
 if (!cancelled) setUserTasks(tasks)
 }

 const loadEvents = async () => {
 const events = await client.get<SpecialEvent>(
 'special_events',
 new QueryBuilder()
 .select('id,title,event_date')
 .eq('organization_id', orgId)
 .gte('event_date', monthStart)
 .lte('event_date', monthEnd)
 .order('event_date', true)
 .limit(8)
 .build(),
 token,
 ).catch(() => [] as SpecialEvent[])
 if (!cancelled) setSpecialEvents(events)
 }

 const loadMessages = async () => {
 if (!userId) return
 // Load task comments where user is recipient
 const comments = await client.get<{ id: string; content: string; created_at: string; author_id: string; task_id: string }>(
 'task_comments',
 new QueryBuilder()
 .select('id,content,created_at,author_id,task_id')
 .eq('organization_id', orgId)
 .order('created_at', false)
 .limit(8)
 .build(),
 token,
 ).catch(() => [] as { id: string; content: string; created_at: string; author_id: string; task_id: string }[])

 if (cancelled) return

 // Fetch author names
 const authorIds = [...new Set(comments.map((c) => c.author_id))]
 let nameMap = new Map<string, string>()
 if (authorIds.length > 0) {
 const users = await client.get<{ id: string; first_name: string; last_name: string }>(
 'users',
 new QueryBuilder().select('id,first_name,last_name').inList('id', authorIds).build(),
 token,
 ).catch(() => [] as { id: string; first_name: string; last_name: string }[])
 nameMap = new Map(users.map((u) => [u.id, `${u.first_name} ${u.last_name}`]))
 }

 if (!cancelled) {
 setUserMessages(
 comments.map((c) => ({
 id: c.id,
 senderName: nameMap.get(c.author_id) || 'Unknown',
 preview: c.content.replace(/^@\w+\s*-?\s*/, '').trim().substring(0, 40) + '...',
 createdAt: c.created_at,
 taskId: c.task_id,
 isRead: true,
 })),
 )
 }
 }

 Promise.all([loadStats(), loadUserTasks(), loadEvents(), loadMessages()]).finally(() => {
 if (!cancelled) setLoading(false)
 })

 return () => { cancelled = true }
 }, [orgId, userId, token, selectedMonth, postgrestUrl, apiKey])

 if (loading) {
 return (
 <div className="min-h-[60vh] flex flex-col items-center justify-center gap-4">
 <Loader2 className="h-10 w-10 animate-spin text-primary" />
 <p className="text-muted-foreground text-lg font-medium animate-pulse">Loading Dashboard</p>
 </div>
 )
 }

 const totalTasks = taskStats.assigned + taskStats.inProgress + taskStats.overdue + taskStats.completed
 const pieData = [
 { name: 'Assigned', value: taskStats.assigned, color: PIE_COLORS[0] },
 { name: 'In-progress', value: taskStats.inProgress, color: PIE_COLORS[1] },
 { name: 'Overdue', value: taskStats.overdue, color: PIE_COLORS[2] },
 { name: 'Completed', value: taskStats.completed, color: PIE_COLORS[3] },
 ].filter((d) => d.value > 0)

 const renderLabel = (entry: { cx: number; cy: number; midAngle: number; innerRadius: number; outerRadius: number; value: number }) => {
 const RADIAN = Math.PI / 180
 const { cx, cy, midAngle, innerRadius, outerRadius, value } = entry
 if (value === 0) return null
 const radius = innerRadius + (outerRadius - innerRadius) * 0.7
 const x = cx + radius * Math.cos(-midAngle * RADIAN)
 const y = cy + radius * Math.sin(-midAngle * RADIAN)
 return (
 <text x={x} y={y} fill="white" textAnchor="middle" dominantBaseline="central" className="font-bold text-base">
 {value}
 </text>
 )
 }

 return (
 <>
 <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
 {/* Workflow Card */}
 <div
 className="cursor-pointer hover:shadow-lg transition-shadow border-2 border-primary rounded-lg bg-card text-card-foreground p-0"
 onClick={() => navigate(`/tasks${user?.id ? `?assignee=${user.id}` : ''}`)}
 >
 <div className="flex items-center justify-between p-4 pb-2">
 <h2 className="text-2xl font-semibold">Workflow</h2>
 <MonthFilter selectedMonth={selectedMonth} onMonthChange={setSelectedMonth} compact />
 </div>
 <div className="px-4 pb-4">
 <h3 className="text-center font-semibold mb-1 text-sm">Tasks ({totalTasks})</h3>
 {totalTasks > 0 ? (
 <ResponsiveContainer width="100%" height={180}>
 <PieChart>
 <Pie data={pieData} cx="50%" cy="50%" labelLine={false} label={renderLabel} outerRadius={65} dataKey="value">
 {pieData.map((entry, i) => (
 <Cell key={i} fill={entry.color} />
 ))}
 </Pie>
 <Legend />
 </PieChart>
 </ResponsiveContainer>
 ) : (
 <div className="h-[180px] flex items-center justify-center text-muted-foreground">No tasks found</div>
 )}
 </div>
 </div>

 {/* Messages Card */}
 <div
 className="cursor-pointer hover:shadow-lg transition-shadow border-2 border-primary rounded-lg bg-card text-card-foreground p-0"
 onClick={() => navigate('/messages')}
 >
 <div className="flex items-center justify-between p-4">
 <h2 className="text-2xl font-semibold">Messages</h2>
 </div>
 <div className="px-4 pb-4 h-[200px] overflow-auto">
 {userMessages.length === 0 ? (
 <div className="text-xs text-muted-foreground text-center py-4">No messages for you</div>
 ) : (
 <div className="space-y-0">
 {userMessages.map((msg) => (
 <div key={msg.id} className="flex justify-between items-start py-1 border-b last:border-0">
 <div className="flex-1 min-w-0">
 <p className="text-xs font-medium">{msg.senderName}</p>
 <p className="text-xs text-muted-foreground truncate">{msg.preview}</p>
 </div>
 <span className="text-xs text-muted-foreground ml-2 whitespace-nowrap">
 {formatMessageTime(msg.createdAt)}
 </span>
 </div>
 ))}
 </div>
 )}
 </div>
 </div>

 {/* My Assignments Card */}
 <div
 className="cursor-pointer hover:shadow-lg transition-shadow border-2 border-primary rounded-lg bg-card text-card-foreground p-0"
 onClick={() => navigate('/tasks')}
 >
 <div className="p-4 pb-2">
 <h2 className="text-2xl font-semibold">My Assignments</h2>
 </div>
 <div className="px-4 pb-4 h-[190px] overflow-auto">
 {userTasks.length === 0 ? (
 <div className="text-xs text-muted-foreground text-center py-4">No tasks assigned to you</div>
 ) : (
 <div className="space-y-0">
 {userTasks.map((task) => (
 <div key={task.id} className="grid grid-cols-[auto_1fr_auto] gap-3 py-1 border-b last:border-0 items-center">
 <span className="text-xs font-medium whitespace-nowrap">
 {new Date(task.due_date).toLocaleDateString()}
 </span>
 <span className="text-xs truncate">{task.title}</span>
 <span className={`text-[10px] px-1.5 py-0 rounded-full font-semibold ${PRIORITY_COLORS[task.priority?.toLowerCase()] || 'bg-gray-100 text-gray-600'}`}>
 {task.priority}
 </span>
 </div>
 ))}
 </div>
 )}
 </div>
 </div>

 {/* Events Card */}
 <div
 className="cursor-pointer hover:shadow-lg transition-shadow border-2 border-primary rounded-lg bg-card text-card-foreground p-0"
 onClick={() => navigate('/special-events')}
 >
 <div className="p-4 pb-2">
 <h2 className="text-2xl font-semibold">Events</h2>
 </div>
 <div className="px-4 pb-4 h-[190px] overflow-auto">
 {specialEvents.length === 0 ? (
 <div className="text-xs text-muted-foreground text-center py-4">No upcoming events</div>
 ) : (
 <div className="space-y-0">
 {specialEvents.map((event) => (
 <div key={event.id} className="grid grid-cols-[auto_1fr] gap-3 py-1 border-b last:border-0">
 <span className="text-xs font-medium whitespace-nowrap">
 {format(new Date(event.event_date + 'T00:00:00'), 'MM-dd-yyyy')}
 </span>
 <span className="text-xs">{event.title}</span>
 </div>
 ))}
 </div>
 )}
 </div>
 </div>
 </div>

 {/* Quick Links */}
 <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mt-4">
  <a
   href="https://monterey-prod-av.accela.com/portlets/web/en-us/#/auth/login"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   ACELLA
  </a>
  <a
   href="https://www.esosuite.net/login/CAS2750675?loggedOut=True&agencyLoginId=CAS2750675"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   ESO
  </a>
  <a
   href="https://portal.tabletcommand.com/login"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   TABLET COMMAND
  </a>
  <a
   href="#"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   Vector Solutions
  </a>
  <button
   onClick={() => navigate('/bpe/timekeeping')}
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   GoTime
  </button>
  <a
   href="https://www.nfpa.org/NFPA-Solutions?gad_source=1&gad_campaignid=19188247804&gbraid=0AAAAAD3hth2roPBLqCGfkMFBORJjqw2sz&gclid=CjwKCAjwuIbBBhBvEiwAsNypvdEemyo2Ii8Yp86yPyZYhPN847K5TbSPiRlIadQGE_STFny5O9VKlxoCZV0QAvD_BwE&gclsrc=aw.ds"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   NFPA
  </a>
  <a
   href="https://www.countyofmonterey.gov/government/about/gis-mapping-data#main_frame"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   GIS Mapping
  </a>
  <a
   href="https://starlink.com"
   target="_blank"
   rel="noopener noreferrer"
   className="bg-primary text-white font-semibold rounded-lg py-3 px-4 text-center text-sm hover:bg-primary/90 transition-colors"
  >
   Starlink
  </a>
 </div>
 </>
 )
}
