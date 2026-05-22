import { useState, useMemo, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { ChevronLeft, ChevronRight, Clock, MapPin, Loader2, CalendarDays, CheckSquare, Grid3X3, LayoutList, Calendar as CalendarIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import type { Task } from '@/models/task'
import type { SpecialEvent } from '@/models/special-event'
import {
 format, isSameDay, startOfWeek, endOfWeek, addDays,
} from 'date-fns'

type ViewMode = 'month' | 'week'

interface UserInfo { id: string; first_name: string; last_name: string }

const MONTH_NAMES = ['January','February','March','April','May','June','July','August','September','October','November','December']
const DAY_HEADERS = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat']

const priorityColors: Record<string, string> = {
 Critical: 'bg-red-500',
 High: 'bg-destructive',
 Medium: 'bg-amber-500',
 Low: 'bg-emerald-500',
}

const priorityChipColors: Record<string, string> = {
 Critical: 'bg-red-500/20 text-red-700',
 High: 'bg-destructive/20 text-destructive',
 Medium: 'bg-amber-500/20 text-amber-700',
 Low: 'bg-emerald-500/20 text-emerald-700',
}

const priorityBadgeColors: Record<string, string> = {
 Critical: 'bg-red-500 text-white',
 High: 'bg-destructive text-destructive-foreground',
 Medium: 'bg-amber-500 text-white',
 Low: 'bg-emerald-500 text-white',
}

function daysInMonth(year: number, month: number) {
 return new Date(year, month + 1, 0).getDate()
}

interface CalendarCell {
 date: string | null
 day: number
 inMonth: boolean
 isToday: boolean
 tasks: Task[]
 events: SpecialEvent[]
}

function buildCells(year: number, month: number, todayStr: string, tasks: Task[], events: SpecialEvent[]): CalendarCell[] {
 const firstDay = new Date(year, month, 1).getDay() // Sun=0
 const monthLen = daysInMonth(year, month)
 const prevMonthLen = daysInMonth(year, month === 0 ? 11 : month - 1)

 const cells: CalendarCell[] = []

 for (let i = 0; i < firstDay; i++) {
 cells.push({ date: null, day: prevMonthLen - firstDay + i + 1, inMonth: false, isToday: false, tasks: [], events: [] })
 }

 for (let d = 1; d <= monthLen; d++) {
 const dateStr = `${year}-${String(month + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`
 cells.push({
 date: dateStr,
 day: d,
 inMonth: true,
 isToday: dateStr === todayStr,
 tasks: tasks.filter((t) => t.due_date === dateStr),
 events: events.filter((e) => e.event_date === dateStr),
 })
 }

 const target = cells.length > 35 ? 42 : 35
 for (let i = 1; cells.length < target; i++) {
 cells.push({ date: null, day: i, inMonth: false, isToday: false, tasks: [], events: [] })
 }

 return cells
}

export function CalendarPage() {
 const navigate = useNavigate()
 const today = new Date()
 const todayStr = today.toISOString().slice(0, 10)
 const [viewYear, setViewYear] = useState(today.getFullYear())
 const [viewMonth, setViewMonth] = useState(today.getMonth())
 const [selectedDate, setSelectedDate] = useState<Date>(today)
 const [viewMode, setViewMode] = useState<ViewMode>('month')

 const token = useAuthStore((s) => s.token)
 const org = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [tasks, setTasks] = useState<Task[]>([])
 const [events, setEvents] = useState<SpecialEvent[]>([])
 const [usersMap, setUsersMap] = useState<Map<string, UserInfo>>(new Map())
 const [loading, setLoading] = useState(true)

 useEffect(() => {
 let cancelled = false
 const doFetch = async () => {
 if (!org) return
 setLoading(true)
 const client = new PostgRestClient(postgrestUrl, apiKey)
 try {
 const [fetchedTasks, fetchedEvents, fetchedUsers] = await Promise.all([
 client.get<Task>(
 'tasks',
 new QueryBuilder()
 .select('id,organization_id,title,status,priority,due_date,assigned_to,created_at')
 .eq('organization_id', org.id)
 .order('due_date', true)
 .limit(200)
 .build(),
 token,
 ),
 client.get<SpecialEvent>(
 'special_events',
 new QueryBuilder()
 .select('id,organization_id,title,description,event_date,event_time,location,created_at')
 .eq('organization_id', org.id)
 .order('event_date', true)
 .limit(100)
 .build(),
 token,
 ),
 client.get<UserInfo>(
 'users',
 new QueryBuilder()
 .select('id,first_name,last_name')
 .eq('organization_id', org.id)
 .limit(200)
 .build(),
 token,
 ),
 ])
 if (!cancelled) {
 setTasks(fetchedTasks)
 setEvents(fetchedEvents)
 setUsersMap(new Map(fetchedUsers.map((u) => [u.id, u])))
 }
 } catch (err) {
 if (!cancelled) console.error('Failed to fetch calendar data', err)
 } finally {
 if (!cancelled) setLoading(false)
 }
 }
 doFetch()
 return () => { cancelled = true }
 }, [org, postgrestUrl, apiKey, token])

 const withDates = useMemo(() => tasks.filter((t) => t.due_date), [tasks])
 const cells = useMemo(() => buildCells(viewYear, viewMonth, todayStr, withDates, events), [viewYear, viewMonth, todayStr, withDates, events])

 // Week view data
 const weekStart = startOfWeek(selectedDate, { weekStartsOn: 0 })
 const weekEnd = endOfWeek(selectedDate, { weekStartsOn: 0 })
 const weekDays = useMemo(() => Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)), [weekStart])

 const getItemsForDate = (date: Date) => {
 const dateStr = format(date, 'yyyy-MM-dd')
 return {
 tasks: withDates.filter((t) => t.due_date === dateStr),
 events: events.filter((e) => e.event_date === dateStr),
 }
 }

 const selectedDateStr = format(selectedDate, 'yyyy-MM-dd')
 const selectedItems = getItemsForDate(selectedDate)

 const getAssigneeName = (task: Task) => {
 if (!task.assigned_to) return 'Unassigned'
 const u = usersMap.get(task.assigned_to)
 return u ? `${u.first_name} ${u.last_name}` : 'Unassigned'
 }

 const prev = () => {
 if (viewMode === 'month') {
 if (viewMonth === 0) { setViewMonth(11); setViewYear(viewYear - 1) } else setViewMonth(viewMonth - 1)
 } else {
 setSelectedDate(addDays(selectedDate, -7))
 }
 }

 const next = () => {
 if (viewMode === 'month') {
 if (viewMonth === 11) { setViewMonth(0); setViewYear(viewYear + 1) } else setViewMonth(viewMonth + 1)
 } else {
 setSelectedDate(addDays(selectedDate, 7))
 }
 }

 const goToday = () => {
 setSelectedDate(today)
 setViewYear(today.getFullYear())
 setViewMonth(today.getMonth())
 }

 const handleTaskClick = (taskId: string) => navigate(`/tasks/${taskId}`)
 const handleEventClick = (eventId: string) => navigate(`/special-events?eventId=${eventId}`)

 if (loading) {
 return (
 <div className="flex items-center justify-center py-24">
 <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
 </div>
 )
 }

 return (
 <div className="space-y-6">
 {/* Header */}
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <CalendarDays className="h-8 w-8 text-primary" />
 <h1 className="text-3xl font-bold">Calendar</h1>
 </div>
 <div className="flex items-center gap-2">
 <Button
 variant={viewMode === 'month' ? 'default' : 'outline'}
 size="sm"
 onClick={() => setViewMode('month')}
 >
 <Grid3X3 className="h-4 w-4 mr-1" />
 Month
 </Button>
 <Button
 variant={viewMode === 'week' ? 'default' : 'outline'}
 size="sm"
 onClick={() => setViewMode('week')}
 >
 <LayoutList className="h-4 w-4 mr-1" />
 Week
 </Button>
 </div>
 </div>

 <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
 {/* Calendar / Week View */}
 <Card className="lg:col-span-2">
 <CardContent className="p-4">
 {viewMode === 'month' ? (
 <>
 {/* Month navigation */}
 <div className="flex items-center justify-between mb-4">
 <Button variant="ghost" size="icon" onClick={prev}>
 <ChevronLeft className="w-5 h-5" />
 </Button>
 <div className="flex items-center gap-2">
 <h2 className="text-lg font-semibold">{MONTH_NAMES[viewMonth]} {viewYear}</h2>
 <Badge variant="secondary" className="cursor-pointer" onClick={goToday}>Today</Badge>
 </div>
 <Button variant="ghost" size="icon" onClick={next}>
 <ChevronRight className="w-5 h-5" />
 </Button>
 </div>

 {/* Day headers */}
 <div className="grid grid-cols-7 mb-1">
 {DAY_HEADERS.map((d) => (
 <div key={d} className="text-center text-xs font-semibold text-muted-foreground py-2">{d}</div>
 ))}
 </div>

 {/* Month grid */}
 <div className="grid grid-cols-7">
 {cells.map((cell, i) => {
 const isSelected = cell.date != null && cell.date === selectedDateStr
 const hasHigh = cell.tasks.some((t) => t.priority === 'High' || t.priority === 'Critical')
 const hasMedium = cell.tasks.some((t) => t.priority === 'Medium')
 const hasLow = cell.tasks.some((t) => t.priority === 'Low')
 const totalItems = cell.tasks.length + cell.events.length

 const bg = isSelected
 ? 'bg-primary/10 border-primary ring-2 ring-primary/40'
 : cell.isToday
 ? 'bg-accent border-primary/50'
 : cell.inMonth
 ? 'bg-card border border-border/50 hover:bg-accent/50'
 : 'bg-muted/30 border border-border/30'

 return (
 <div
 key={i}
 className={`rounded-md p-1 min-h-[96px] ${bg} ${cell.inMonth ? 'cursor-pointer transition-colors' : ''} overflow-hidden`}
 onClick={() => {
 if (cell.date) {
 setSelectedDate(new Date(cell.date + 'T00:00:00'))
 }
 }}
 >
 <div className="flex flex-col h-full">
 <span className={`text-sm mb-1 ${cell.isToday ? 'font-bold text-primary' : cell.inMonth ? 'text-foreground' : 'text-muted-foreground/50'}`}>
 {cell.day}
 </span>
 <div className="flex flex-wrap gap-0.5 mt-auto">
 {hasHigh && <div className="w-2 h-2 rounded-full bg-destructive" title="High priority" />}
 {hasMedium && <div className="w-2 h-2 rounded-full bg-amber-500" title="Medium priority" />}
 {hasLow && <div className="w-2 h-2 rounded-full bg-emerald-500" title="Low priority" />}
 {cell.events.length > 0 && <div className="w-2 h-2 rounded-full bg-primary" title="Event" />}
 </div>
 {totalItems > 0 && (
 <span className="text-[10px] text-muted-foreground mt-0.5">
 {totalItems} item{totalItems > 1 ? 's' : ''}
 </span>
 )}
 </div>
 </div>
 )
 })}
 </div>
 </>
 ) : (
 /* Week View */
 <div className="space-y-4">
 <div className="flex items-center justify-between">
 <Button variant="outline" size="sm" onClick={prev}>
 <ChevronLeft className="h-4 w-4 mr-1" />
 Previous Week
 </Button>
 <div className="flex items-center gap-2">
 <h2 className="text-lg font-semibold">
 {format(weekStart, 'MMM d')} - {format(weekEnd, 'MMM d, yyyy')}
 </h2>
 <Badge variant="secondary" className="cursor-pointer" onClick={goToday}>Today</Badge>
 </div>
 <Button variant="outline" size="sm" onClick={next}>
 Next Week
 <ChevronRight className="h-4 w-4 ml-1" />
 </Button>
 </div>
 <div className="grid grid-cols-7 gap-2">
 {weekDays.map((day) => {
 const items = getItemsForDate(day)
 const isSelected = isSameDay(day, selectedDate)
 const isToday = isSameDay(day, today)

 return (
 <div
 key={day.toISOString()}
 className={`border rounded-md p-2 min-h-[320px] cursor-pointer transition-colors ${
 isSelected ? 'border-primary bg-primary/5' : 'border-border hover:bg-accent/50'
 } ${isToday ? 'bg-accent/30' : ''}`}
 onClick={() => setSelectedDate(day)}
 >
 <div className="text-center mb-2">
 <div className="text-xs text-muted-foreground">{format(day, 'EEE')}</div>
 <div className={`text-lg font-semibold ${isToday ? 'text-primary' : ''}`}>
 {format(day, 'd')}
 </div>
 </div>
 <div className="space-y-1 overflow-y-auto max-h-[260px]">
 {items.tasks.slice(0, 6).map((task) => (
 <div
 key={task.id}
 className={`text-[10px] p-1 rounded truncate cursor-pointer hover:opacity-80 ${
 priorityChipColors[task.priority ?? ''] ?? 'bg-blue-500/20 text-blue-700'
 }`}
 onClick={(e) => { e.stopPropagation(); handleTaskClick(task.id) }}
 >
 {task.title}
 </div>
 ))}
 {items.events.slice(0, Math.max(0, 8 - items.tasks.slice(0, 6).length)).map((event) => (
 <div
 key={event.id}
 className="text-[10px] p-1 rounded bg-primary/20 text-primary truncate cursor-pointer hover:opacity-80"
 onClick={(e) => { e.stopPropagation(); handleEventClick(event.id) }}
 >
 {event.title}
 </div>
 ))}
 {items.tasks.length + items.events.length > 8 && (
 <div className="text-[10px] text-muted-foreground text-center">
 +{items.tasks.length + items.events.length - 8} more
 </div>
 )}
 </div>
 </div>
 )
 })}
 </div>
 </div>
 )}
 </CardContent>
 </Card>

 {/* Selected Date Details */}
 <Card>
 <CardHeader className="pb-3">
 <CardTitle className="text-lg flex items-center gap-2">
 <CalendarIcon className="h-5 w-5 text-primary" />
 {format(selectedDate, 'MMMM d, yyyy')}
 </CardTitle>
 </CardHeader>
 <CardContent className="space-y-4">
 {/* Legend */}
 <div className="flex flex-wrap gap-3 pb-3 border-b border-border">
 <div className="flex items-center gap-1.5 text-xs">
 <div className="w-3 h-3 rounded-full bg-destructive" />
 <span>High</span>
 </div>
 <div className="flex items-center gap-1.5 text-xs">
 <div className="w-3 h-3 rounded-full bg-amber-500" />
 <span>Medium</span>
 </div>
 <div className="flex items-center gap-1.5 text-xs">
 <div className="w-3 h-3 rounded-full bg-emerald-500" />
 <span>Low</span>
 </div>
 <div className="flex items-center gap-1.5 text-xs">
 <div className="w-3 h-3 rounded-full bg-primary" />
 <span>Event</span>
 </div>
 </div>

 {/* Tasks */}
 {selectedItems.tasks.length > 0 && (
 <div>
 <h3 className="text-sm font-semibold flex items-center gap-2 mb-2">
 <CheckSquare className="h-4 w-4" />
 Tasks ({selectedItems.tasks.length})
 </h3>
 <div className="space-y-2 max-h-48 overflow-y-auto">
 {selectedItems.tasks.map((task) => (
 <div
 key={task.id}
 className="p-2 rounded-md bg-muted/50 border border-border/50 cursor-pointer hover:bg-muted transition-colors"
 onClick={() => handleTaskClick(task.id)}
 >
 <div className="flex items-start justify-between gap-2">
 <span className="text-sm font-medium line-clamp-2">{task.title}</span>
 <Badge className={`text-[10px] shrink-0 ${priorityBadgeColors[task.priority ?? ''] ?? 'bg-blue-400 text-white'}`}>
 {task.priority ?? 'Low'}
 </Badge>
 </div>
 <div className="flex items-center gap-2 mt-1">
 <Badge variant="outline" className="text-[10px]">{task.status}</Badge>
 <span className="text-[10px] text-muted-foreground">{getAssigneeName(task)}</span>
 </div>
 </div>
 ))}
 </div>
 </div>
 )}

 {/* Events */}
 {selectedItems.events.length > 0 && (
 <div>
 <h3 className="text-sm font-semibold flex items-center gap-2 mb-2">
 <CalendarDays className="h-4 w-4 text-primary" />
 Events ({selectedItems.events.length})
 </h3>
 <div className="space-y-2 max-h-48 overflow-y-auto">
 {selectedItems.events.map((event) => (
 <div
 key={event.id}
 className="p-2 rounded-md bg-primary/10 border border-primary/20 cursor-pointer hover:bg-primary/20 transition-colors"
 onClick={() => handleEventClick(event.id)}
 >
 <span className="text-sm font-medium line-clamp-2">{event.title}</span>
 {event.event_time && (
 <p className="text-xs text-muted-foreground mt-1 flex items-center gap-1">
 <Clock className="w-3 h-3" />{event.event_time}
 </p>
 )}
 {event.location && (
 <p className="text-xs text-muted-foreground flex items-center gap-1">
 <MapPin className="w-3 h-3" />{event.location}
 </p>
 )}
 </div>
 ))}
 </div>
 </div>
 )}

 {/* Empty state */}
 {selectedItems.tasks.length === 0 && selectedItems.events.length === 0 && (
 <p className="text-sm text-muted-foreground text-center py-4">
 No tasks or events scheduled for this date.
 </p>
 )}
 </CardContent>
 </Card>
 </div>
 </div>
 )
}
