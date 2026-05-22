import { useState, useEffect, useCallback, useMemo } from 'react'
import {
 Dialog,
 DialogContent,
 DialogHeader,
 DialogTitle,
 DialogFooter,
} from '@/components/ui/dialog'
import {
 AlertDialog,
 AlertDialogAction,
 AlertDialogCancel,
 AlertDialogContent,
 AlertDialogDescription,
 AlertDialogFooter,
 AlertDialogHeader,
 AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Badge } from '@/components/ui/badge'
import { Card } from '@/components/ui/card'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import {
 Loader2,
 Plus,
 Pencil,
 Trash2,
 Clock,
 MapPin,
 Sparkles,
 CalendarDays,
} from 'lucide-react'
import type { SpecialEvent } from '@/models/special-event'

const MONTH_NAMES = [
 'January', 'February', 'March', 'April', 'May', 'June',
 'July', 'August', 'September', 'October', 'November', 'December',
]
const DAY_NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']

export function SpecialEventsPage() {
 const token = useAuthStore((s) => s.token)
 const user = useAuthStore((s) => s.user)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')

 const [events, setEvents] = useState<SpecialEvent[]>([])
 const [loading, setLoading] = useState(true)
 const [dialogOpen, setDialogOpen] = useState(false)
 const [editingEvent, setEditingEvent] = useState<SpecialEvent | null>(null)
 const [saving, setSaving] = useState(false)
 const [deleteTarget, setDeleteTarget] = useState<SpecialEvent | null>(null)

 // Form state
 const [formTitle, setFormTitle] = useState('')
 const [formDescription, setFormDescription] = useState('')
 const [formDate, setFormDate] = useState('')
 const [formTime, setFormTime] = useState('')
 const [formLocation, setFormLocation] = useState('')

 const fetchEvents = useCallback(async () => {
 if (!orgId) return
 setLoading(true)
 try {
 const { postgrestUrl, apiKey } = detectApiUrls()
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const query = new QueryBuilder()
 .select(
 'id,organization_id,title,description,event_date,event_time,location,created_by,created_at',
 )
 .eq('organization_id', orgId)
 .order('event_date', true)
 .limit(50)
 .build()
 const data = await client.get<SpecialEvent>('special_events', query, token)
 setEvents(data)
 } catch (err) {
 toast.error('Failed to load events')
 console.error(err)
 } finally {
 setLoading(false)
 }
 }, [orgId, token])

 useEffect(() => {
 fetchEvents()
 }, [fetchEvents])

 const groups = useMemo(() => {
 const map = new Map<string, SpecialEvent[]>()
 for (const ev of events) {
 const d = ev.event_date ? new Date(ev.event_date + 'T00:00:00') : null
 const key = d
 ? `${MONTH_NAMES[d.getMonth()]} ${d.getFullYear()}`
 : 'Unscheduled'
 const arr = map.get(key) ?? []
 arr.push(ev)
 map.set(key, arr)
 }
 return [...map.entries()]
 }, [events])

 function resetForm() {
 setFormTitle('')
 setFormDescription('')
 setFormDate('')
 setFormTime('')
 setFormLocation('')
 }

 function openCreateDialog() {
 setEditingEvent(null)
 resetForm()
 setDialogOpen(true)
 }

 function openEditDialog(ev: SpecialEvent) {
 setEditingEvent(ev)
 setFormTitle(ev.title)
 setFormDescription(ev.description ?? '')
 setFormDate(ev.event_date ?? '')
 setFormTime(ev.event_time ?? '')
 setFormLocation(ev.location ?? '')
 setDialogOpen(true)
 }

 async function handleSave() {
 if (!formTitle.trim()) {
 toast.error('Title is required')
 return
 }
 if (!orgId) return

 setSaving(true)
 try {
 const { postgrestUrl, apiKey } = detectApiUrls()
 const client = new PostgRestClient(postgrestUrl, apiKey)

 if (editingEvent) {
 const query = new QueryBuilder().eq('id', editingEvent.id).build()
 await client.patch<SpecialEvent>('special_events', query, {
 title: formTitle.trim(),
 description: formDescription.trim() || null,
 event_date: formDate || null,
 event_time: formTime || null,
 location: formLocation.trim() || null,
 }, token)
 toast.success('Event updated')
 } else {
 await client.post<SpecialEvent>('special_events', {
 organization_id: orgId,
 title: formTitle.trim(),
 description: formDescription.trim() || null,
 event_date: formDate || null,
 event_time: formTime || null,
 location: formLocation.trim() || null,
 created_by: user?.id ?? null,
 }, token)
 toast.success('Event created')
 }

 setDialogOpen(false)
 fetchEvents()
 } catch (err) {
 toast.error(editingEvent ? 'Failed to update event' : 'Failed to create event')
 console.error(err)
 } finally {
 setSaving(false)
 }
 }

 async function handleDelete(ev: SpecialEvent) {
 try {
 const { postgrestUrl, apiKey } = detectApiUrls()
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const query = new QueryBuilder().eq('id', ev.id).build()
 await client.delete('special_events', query, token)
 toast.success('Event deleted')
 setDeleteTarget(null)
 fetchEvents()
 } catch (err) {
 toast.error('Failed to delete event')
 console.error(err)
 }
 }

 return (
 <div className="space-y-6">
 {/* Header */}
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <Sparkles className="h-7 w-7 text-purple-500" />
 <h1 className="text-2xl font-bold text-foreground">Special Events</h1>
 {!loading && <Badge variant="secondary">{events.length} events</Badge>}
 </div>
 <Button size="sm" onClick={openCreateDialog}>
 <Plus className="mr-1 h-4 w-4" />
 Add Event
 </Button>
 </div>

 {/* Loading */}
 {loading && (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
 </div>
 )}

 {/* Empty state */}
 {!loading && events.length === 0 && (
 <div className="text-center py-12">
 <CalendarDays className="mx-auto h-10 w-10 text-muted-foreground/50" />
 <p className="mt-3 text-muted-foreground">No events scheduled yet.</p>
 <Button variant="outline" size="sm" className="mt-4" onClick={openCreateDialog}>
 <Plus className="mr-1 h-4 w-4" />
 Create your first event
 </Button>
 </div>
 )}

 {/* Month groups */}
 {groups.map(([label, monthEvents]) => (
 <div key={label} className="space-y-3">
 <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider sticky top-0 bg-background py-1 z-10">
 {label}
 </h2>
 <div className="space-y-3">
 {monthEvents.map((ev) => {
 const d = ev.event_date
 ? new Date(ev.event_date + 'T00:00:00')
 : null
 const day = d ? d.getDate().toString() : '?'
 const weekday = d ? DAY_NAMES[d.getDay()] : ''
 return (
 <Card
 key={ev.id}
 className="p-4 flex gap-4 hover:shadow-md hover:border-purple-300 transition-all"
 >
 {/* Date badge */}
 <div className="w-14 h-14 rounded-lg bg-purple-50 border border-purple-200 flex flex-col items-center justify-center shrink-0">
 <span className="text-lg font-bold text-purple-700 leading-none">
 {day}
 </span>
 <span className="text-[10px] font-medium text-purple-500 uppercase">
 {weekday}
 </span>
 </div>

 {/* Content */}
 <div className="flex-1 min-w-0">
 <p className="font-medium text-foreground">{ev.title}</p>
 <div className="flex flex-wrap items-center gap-3 mt-1.5 text-xs text-muted-foreground">
 <span className="flex items-center gap-1">
 <Clock className="h-3.5 w-3.5" />
 {ev.event_time ?? 'All day'}
 </span>
 <span className="flex items-center gap-1">
 <MapPin className="h-3.5 w-3.5" />
 {ev.location ?? 'TBD'}
 </span>
 </div>
 {ev.description && (
 <p className="text-xs text-muted-foreground/70 mt-1.5 line-clamp-2">
 {ev.description}
 </p>
 )}
 </div>

 {/* Actions */}
 <div className="flex items-start gap-1 shrink-0">
 <Button
 variant="ghost"
 size="icon"
 className="h-8 w-8"
 onClick={() => openEditDialog(ev)}
 >
 <Pencil className="h-4 w-4" />
 </Button>
 <Button
 variant="ghost"
 size="icon"
 className="h-8 w-8 text-destructive hover:text-destructive"
 onClick={() => setDeleteTarget(ev)}
 >
 <Trash2 className="h-4 w-4" />
 </Button>
 </div>
 </Card>
 )
 })}
 </div>
 </div>
 ))}

 {/* Create / Edit Dialog */}
 <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
 <DialogContent>
 <DialogHeader>
 <DialogTitle>
 {editingEvent ? 'Edit Event' : 'Add Event'}
 </DialogTitle>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="event-title">Title</Label>
 <Input
 id="event-title"
 placeholder="Event title"
 value={formTitle}
 onChange={(e) => setFormTitle(e.target.value)}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="event-description">Description</Label>
 <Textarea
 id="event-description"
 placeholder="Optional description"
 value={formDescription}
 onChange={(e) => setFormDescription(e.target.value)}
 rows={3}
 />
 </div>
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-2">
 <Label htmlFor="event-date">Date</Label>
 <Input
 id="event-date"
 type="date"
 value={formDate}
 onChange={(e) => setFormDate(e.target.value)}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="event-time">Time</Label>
 <Input
 id="event-time"
 type="time"
 value={formTime}
 onChange={(e) => setFormTime(e.target.value)}
 />
 </div>
 </div>
 <div className="space-y-2">
 <Label htmlFor="event-location">Location</Label>
 <Input
 id="event-location"
 placeholder="Optional location"
 value={formLocation}
 onChange={(e) => setFormLocation(e.target.value)}
 />
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setDialogOpen(false)}>
 Cancel
 </Button>
 <Button onClick={handleSave} disabled={saving}>
 {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
 {editingEvent ? 'Save Changes' : 'Create Event'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete Confirmation */}
 <AlertDialog
 open={!!deleteTarget}
 onOpenChange={(open) => {
 if (!open) setDeleteTarget(null)
 }}
 >
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>Delete Event</AlertDialogTitle>
 <AlertDialogDescription>
 Are you sure you want to delete &quot;{deleteTarget?.title}&quot;? This
 action cannot be undone.
 </AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel>Cancel</AlertDialogCancel>
 <AlertDialogAction
 onClick={() => deleteTarget && handleDelete(deleteTarget)}
 >
 Delete
 </AlertDialogAction>
 </AlertDialogFooter>
 </AlertDialogContent>
 </AlertDialog>
 </div>
 )
}
