import { useState, useEffect, useCallback, useMemo } from 'react'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import {
 Loader2,
 ChevronLeft,
 ChevronRight,
 Calendar,
 Lock,
 Unlock,
 RefreshCw,
 Users,
 CalendarDays,
 AlertTriangle,
 CheckCircle2,
 Shield,
 Pencil,
} from 'lucide-react'

// ── Types ──

interface RosterAssignment {
 id?: string
 employee_id?: string
 employee_name: string
 employee_rank: string
 employee_shift?: string
 station_id?: string
 station_name: string
 assignment_type: string
 is_cism_coverage: boolean
 is_24hr: boolean
 notes?: string
}

interface RosterAbsence {
 id?: string
 employee_id?: string
 employee_name: string
 absence_type: string
}

interface RosterData {
 id?: string
 shift_label: string
 duty_chief_id?: string
 duty_chief_name: string
 assignments: RosterAssignment[]
 absences: RosterAbsence[]
 is_locked: boolean
}

interface RosterRangeDay {
 roster_date: string
 shift_label: string
 assignment_count: number
 absence_count: number
}

interface GenerateResult {
 created: number
 skipped: number
 alerts: string[]
}

// ── Helpers ──

const SHIFT_COLORS: Record<string, { bg: string; text: string; border: string }> = {
 A: { bg: 'bg-red-100', text: 'text-red-700', border: 'border-red-300' },
 B: { bg: 'bg-blue-100', text: 'text-blue-700', border: 'border-blue-300' },
 C: { bg: 'bg-green-100', text: 'text-green-700', border: 'border-green-300' },
}

const RANK_BADGE_CLASSES: Record<string, string> = {
 captain: 'bg-red-600 text-white',
 lieutenant: 'bg-red-500 text-white',
 engineer: 'bg-red-400 text-white',
 firefighter: 'bg-blue-600 text-white',
 'firefighter/paramedic': 'bg-blue-600 text-white',
 probation: 'bg-purple-600 text-white',
 probationary: 'bg-purple-600 text-white',
 ot: 'bg-green-600 text-white',
 overtime: 'bg-green-600 text-white',
 reserve: 'bg-yellow-500 text-gray-900',
}

function rankBadgeClass(rank: string, assignmentType: string): string {
 const lower = assignmentType.toLowerCase()
 if (lower === 'ot' || lower === 'overtime') return RANK_BADGE_CLASSES['ot']
 if (lower === 'reserve') return RANK_BADGE_CLASSES['reserve']
 const key = rank.toLowerCase()
 for (const [k, v] of Object.entries(RANK_BADGE_CLASSES)) {
 if (key.includes(k)) return v
 }
 return 'bg-gray-500 text-white'
}

function formatDate(d: Date): string {
 return d.toISOString().slice(0, 10)
}

function formatDisplayDate(d: Date): string {
 return d.toLocaleDateString('en-US', { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' })
}

function addDays(d: Date, n: number): Date {
 const r = new Date(d)
 r.setDate(r.getDate() + n)
 return r
}

const ABSENCE_TYPES = ['Vacation', 'Sick', 'Light Duty', 'Worker Comp', 'Trade'] as const

const STATION_NAMES = ['Station 1', 'Station 2', 'Station 3'] as const

// ── Component ──

export function ShiftRosterPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [selectedDate, setSelectedDate] = useState<Date>(new Date())
 const [roster, setRoster] = useState<RosterData | null>(null)
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Calendar view
 const [calendarView, setCalendarView] = useState(false)
 const [rangeData, setRangeData] = useState<RosterRangeDay[]>([])
 const [rangeLoading, setRangeLoading] = useState(false)

 // Generate roster
 const [showGenerate, setShowGenerate] = useState(false)
 const [genStart, setGenStart] = useState(formatDate(new Date()))
 const [genEnd, setGenEnd] = useState(formatDate(addDays(new Date(), 30)))
 const [generating, setGenerating] = useState(false)
 const [genResult, setGenResult] = useState<GenerateResult | null>(null)

 // Shift view filter (all 3 shifts run each day)
 const [viewShift, setViewShift] = useState('A')

 // Edit mode
 const [editMode, setEditMode] = useState(false)
 const [locking, setLocking] = useState(false)
 const [saving, setSaving] = useState(false)

 // Editable assignments (cloned from roster when entering edit mode)
 const [editAssignments, setEditAssignments] = useState<RosterAssignment[]>([])
 const [originalAssignments, setOriginalAssignments] = useState<RosterAssignment[]>([])
 const [editDutyChiefId, setEditDutyChiefId] = useState<string | null>(null)

 // Employees & stations for dropdowns
 const [allEmployees, setAllEmployees] = useState<{ id: string; name: string; rank: string; shift: string }[]>([])
 const [allStations, setAllStations] = useState<{ id: string; name: string; station_number: number }[]>([])

 // Add absence in edit mode
 const [showAddAbsence, setShowAddAbsence] = useState(false)
 const [newAbsenceEmpId, setNewAbsenceEmpId] = useState('')
 const [newAbsenceType, setNewAbsenceType] = useState('Vacation')

 const client = useMemo(() => (token ? new BpeClient(token) : null), [token])

 // ── Fetch daily roster ──

 const fetchRoster = useCallback(async () => {
 if (!client || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const res = await client.tkGetRoster(orgSlug, formatDate(selectedDate))
 const data = res.data as RosterData
 setRoster(data)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load roster')
 setRoster(null)
 } finally {
 setLoading(false)
 }
 }, [client, orgSlug, selectedDate])

 useEffect(() => {
 if (!calendarView) fetchRoster()
 }, [fetchRoster, calendarView])

 // Load employees/stations on mount for shift filtering
 useEffect(() => {
 loadEmployeesIfNeeded()
 // eslint-disable-next-line react-hooks/exhaustive-deps
 }, [client, orgSlug])

 // ── Fetch calendar range ──

 const fetchRange = useCallback(async () => {
 if (!client || !orgSlug) return
 setRangeLoading(true)
 const monthStart = new Date(selectedDate.getFullYear(), selectedDate.getMonth(), 1)
 const monthEnd = new Date(selectedDate.getFullYear(), selectedDate.getMonth() + 1, 0)
 try {
 const res = await client.tkGetRosterRange(orgSlug, formatDate(monthStart), formatDate(monthEnd))
 setRangeData(res.data as RosterRangeDay[])
 } catch {
 setRangeData([])
 } finally {
 setRangeLoading(false)
 }
 }, [client, orgSlug, selectedDate])

 useEffect(() => {
 if (calendarView) fetchRange()
 }, [fetchRange, calendarView])

 // ── Generate roster ──

 const handleGenerate = async () => {
 if (!client || !orgSlug) return
 setGenerating(true)
 setGenResult(null)
 try {
 const res = await client.tkGenerateRoster({
 organization_id: orgSlug,
 start: genStart,
 end: genEnd,
 })
 setGenResult(res)
 fetchRoster()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to generate roster')
 } finally {
 setGenerating(false)
 }
 }

 // ── Load employees/stations if needed (for dropdowns outside edit mode) ──

 const loadEmployeesIfNeeded = async () => {
 if (allEmployees.length > 0 || !client || !orgSlug) return
 try {
 const [empRes, stRes] = await Promise.all([
 client.tkListEmployees(orgSlug, { status: 'active', per_page: '200' }),
 client.tkListStations(orgSlug),
 ])
 setAllEmployees(
 ((empRes.data ?? []) as { id: string; first_name: string; last_name: string; rank: string; shift_assignment: string }[])
 .map((e) => ({ id: e.id, name: `${e.first_name} ${e.last_name}`, rank: e.rank, shift: e.shift_assignment ?? '' }))
 .sort((a, b) => a.name.localeCompare(b.name)),
 )
 setAllStations(
 ((stRes.data ?? []) as { id: string; name: string; station_number: number }[])
 .sort((a, b) => a.station_number - b.station_number),
 )
 } catch { /* non-critical */ }
 }

 // ── Enter/exit edit mode ──

 const enterEditMode = async () => {
 if (!client || !orgSlug || !roster) return
 setEditAssignments([...roster.assignments])
 setOriginalAssignments([...roster.assignments])
 setEditDutyChiefId(null)
 setEditMode(true)
 await loadEmployeesIfNeeded()
 }

 const exitEditMode = () => {
 setEditMode(false)
 setEditAssignments([])
 setShowAddAbsence(false)
 }

 // ── Save edited assignments ──

 const handleSaveAssignments = async () => {
 if (!client || !roster?.id || !orgSlug) return
 setSaving(true)
 try {
 const payload = {
 assignments: editAssignments.map((a) => ({
 employee_id: a.employee_id,
 station_id: a.station_id || null,
 assignment_type: a.assignment_type,
 is_cism_coverage: a.is_cism_coverage,
 is_24hr: a.is_24hr,
 notes: a.notes || null,
 })),
 }
 await client.tkUpdateAssignments(roster.id, payload)

 // Update duty chief if changed
 if (editDutyChiefId) {
 await client.tkUpdateRoster(roster.id, { duty_chief_id: editDutyChiefId })
 }

 exitEditMode()
 fetchRoster()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to save assignments')
 } finally {
 setSaving(false)
 }
 }

 // ── Add/remove assignment in edit mode ──

 const addAssignment = (empId: string, stationId: string) => {
 const emp = allEmployees.find((e) => e.id === empId)
 const stn = allStations.find((s) => s.id === stationId)
 if (!emp) return
 // Don't add duplicate
 if (editAssignments.some((a) => a.employee_id === empId)) return
 setEditAssignments((prev) => [
 ...prev,
 {
 employee_id: empId,
 employee_name: emp.name,
 employee_rank: emp.rank,
 employee_shift: emp.shift?.toUpperCase() ?? '',
 station_id: stationId,
 station_name: stn?.name ?? 'Unassigned',
 assignment_type: 'regular',
 is_cism_coverage: false,
 is_24hr: false,
 },
 ])
 }

 const removeAssignment = (empId: string) => {
 setEditAssignments((prev) => prev.filter((a) => a.employee_id !== empId))
 }

 const moveToStation = (empId: string, stationId: string) => {
 const stn = allStations.find((s) => s.id === stationId)
 setEditAssignments((prev) =>
 prev.map((a) =>
 a.employee_id === empId ? { ...a, station_id: stationId, station_name: stn?.name ?? 'Unassigned' } : a,
 ),
 )
 }

 const toggleCism = (empId: string) => {
 setEditAssignments((prev) =>
 prev.map((a) => (a.employee_id === empId ? { ...a, is_cism_coverage: !a.is_cism_coverage } : a)),
 )
 }

 const toggle24hr = (empId: string) => {
 setEditAssignments((prev) =>
 prev.map((a) => (a.employee_id === empId ? { ...a, is_24hr: !a.is_24hr } : a)),
 )
 }

 const setAssignmentType = (empId: string, type: string) => {
 setEditAssignments((prev) =>
 prev.map((a) => (a.employee_id === empId ? { ...a, assignment_type: type } : a)),
 )
 }

 // ── Add absence ──

 const handleAddAbsence = async () => {
 if (!client || !orgSlug || !newAbsenceEmpId) return
 try {
 await client.tkCreateAbsence({
 organization_id: orgSlug,
 employee_id: newAbsenceEmpId,
 absence_date: formatDate(selectedDate),
 absence_type: newAbsenceType,
 })
 setNewAbsenceEmpId('')
 setShowAddAbsence(false)
 fetchRoster()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to add absence')
 }
 }

 const handleDeleteAbsence = async (absenceId: string) => {
 if (!client) return
 try {
 await client.tkDeleteAbsence(absenceId)
 fetchRoster()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to remove absence')
 }
 }

 // ── Lock roster ──

 const handleLock = async () => {
 if (!client || !roster?.id) return
 setLocking(true)
 try {
 await client.tkLockRoster(roster.id)
 fetchRoster()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to lock roster')
 } finally {
 setLocking(false)
 }
 }

 const handleUnlock = async () => {
 if (!client || !roster?.id) return
 setLocking(true)
 try {
 await client.tkUnlockRoster(roster.id)
 fetchRoster()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to unlock roster')
 } finally {
 setLocking(false)
 }
 }

 // ── Date navigation ──

 const goToday = () => setSelectedDate(new Date())
 const goPrev = () => setSelectedDate((d) => addDays(d, -1))
 const goNext = () => setSelectedDate((d) => addDays(d, 1))

 // ── Derived data ──

 const shiftStyle = SHIFT_COLORS[viewShift] ?? SHIFT_COLORS['A']

 const allAssignments = editMode ? editAssignments : (roster?.assignments ?? [])

 // Determine effective duty chief ID (edit mode may have a pending change)
 const effectiveDutyChiefId = editMode ? (editDutyChiefId ?? roster?.duty_chief_id) : roster?.duty_chief_id

 // Filter assignments by viewShift, and exclude the duty chief from station lists
 const activeAssignments = useMemo(() => {
 return allAssignments.filter((a) => {
 // Exclude duty chief from station personnel
 if (effectiveDutyChiefId && a.employee_id === effectiveDutyChiefId) return false
 if (a.is_24hr) return true // 24hr staff appear in all shifts
 const empShift = (a.employee_shift ?? '').toUpperCase()
 if (!empShift) return true // admin/unassigned — show in all shifts
 return empShift === viewShift
 })
 }, [allAssignments, viewShift, effectiveDutyChiefId])

 // Detect which stations have been edited
 const editedStations = useMemo(() => {
 if (!editMode) return new Set<string>()
 const edited = new Set<string>()
 for (const station of STATION_NAMES) {
 const origForStation = originalAssignments.filter((a) => a.station_name === station)
 const editForStation = editAssignments.filter((a) => a.station_name === station)
 // Compare by employee IDs, assignment types, 24hr, cism
 const origKey = origForStation.map((a) => `${a.employee_id}:${a.assignment_type}:${a.is_24hr}:${a.is_cism_coverage}`).sort().join('|')
 const editKey = editForStation.map((a) => `${a.employee_id}:${a.assignment_type}:${a.is_24hr}:${a.is_cism_coverage}`).sort().join('|')
 if (origKey !== editKey) edited.add(station)
 }
 return edited
 }, [editMode, originalAssignments, editAssignments])

 const stationAssignments = useMemo(() => {
 const grouped: Record<string, RosterAssignment[]> = {}
 for (const a of activeAssignments) {
 const key = a.station_name || 'Unassigned'
 if (!grouped[key]) grouped[key] = []
 grouped[key].push(a)
 }
 return grouped
 }, [activeAssignments])

 const cismPersonnel = useMemo(
 () => activeAssignments.filter((a) => a.is_cism_coverage),
 [activeAssignments],
 )

 // Employees not currently assigned (for add dropdown)
 const unassignedEmployees = useMemo(() => {
 const assignedIds = new Set(editAssignments.map((a) => a.employee_id))
 return allEmployees.filter((e) => !assignedIds.has(e.id))
 }, [allEmployees, editAssignments])

 const groupedAbsences = useMemo(() => {
 if (!roster) return {}
 const grouped: Record<string, string[]> = {}
 for (const a of roster.absences) {
 const type = a.absence_type || 'Other'
 if (!grouped[type]) grouped[type] = []
 grouped[type].push(a.employee_name)
 }
 return grouped
 }, [roster])

 // ── Calendar range data map ──

 const rangeMap = useMemo(() => {
 const m = new Map<string, RosterRangeDay>()
 for (const d of rangeData) m.set(d.roster_date, d)
 return m
 }, [rangeData])

 // ── Render guards ──

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view the shift roster.</p>
 </div>
 )
 }

 // ── Calendar month view ──

 function renderCalendar() {
 const year = selectedDate.getFullYear()
 const month = selectedDate.getMonth()
 const firstDay = new Date(year, month, 1).getDay()
 const daysInMonth = new Date(year, month + 1, 0).getDate()
 const weeks: (number | null)[][] = []
 let week: (number | null)[] = Array(firstDay).fill(null)

 for (let d = 1; d <= daysInMonth; d++) {
 week.push(d)
 if (week.length === 7) {
 weeks.push(week)
 week = []
 }
 }
 if (week.length > 0) {
 while (week.length < 7) week.push(null)
 weeks.push(week)
 }

 const monthLabel = new Date(year, month).toLocaleDateString('en-US', { month: 'long', year: 'numeric' })

 return (
 <Card>
 <CardHeader className="pb-2">
 <div className="flex items-center justify-between">
 <Button
 variant="ghost"
 size="sm"
 onClick={() => setSelectedDate(new Date(year, month - 1, 1))}
 >
 <ChevronLeft className="w-4 h-4" />
 </Button>
 <CardTitle className="text-lg">{monthLabel}</CardTitle>
 <Button
 variant="ghost"
 size="sm"
 onClick={() => setSelectedDate(new Date(year, month + 1, 1))}
 >
 <ChevronRight className="w-4 h-4" />
 </Button>
 </div>
 </CardHeader>
 <CardContent>
 {rangeLoading ? (
 <div className="flex items-center justify-center h-48">
 <Loader2 className="w-6 h-6 animate-spin text-indigo-500" />
 </div>
 ) : (
 <table className="w-full text-sm">
 <thead>
 <tr>
 {['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'].map((d) => (
 <th key={d} className="py-2 text-center text-gray-500 font-medium">
 {d}
 </th>
 ))}
 </tr>
 </thead>
 <tbody>
 {weeks.map((wk, wi) => (
 <tr key={wi}>
 {wk.map((day, di) => {
 if (day === null) return <td key={di} />
 const dateStr = formatDate(new Date(year, month, day))
 const entry = rangeMap.get(dateStr)
 const shiftColor = entry ? SHIFT_COLORS[entry.shift_label] : null
 const isToday = dateStr === formatDate(new Date())
 return (
 <td
 key={di}
 className={`p-1 text-center cursor-pointer rounded-lg hover:bg-gray-100 ${isToday ? 'ring-2 ring-indigo-500' : ''}`}
 onClick={() => {
 setSelectedDate(new Date(year, month, day))
 setCalendarView(false)
 }}
 >
 <div className="text-xs font-medium">{day}</div>
 {entry && (
 <div className={`mt-0.5 rounded px-1 py-0.5 text-[10px] font-bold ${shiftColor?.bg ?? ''} ${shiftColor?.text ?? ''}`}>
 {entry.shift_label} &middot; {entry.assignment_count}
 {entry.absence_count > 0 && (
 <span className="ml-0.5 text-amber-600">-{entry.absence_count}</span>
 )}
 </div>
 )}
 </td>
 )
 })}
 </tr>
 ))}
 </tbody>
 </table>
 )}
 </CardContent>
 </Card>
 )
 }

 // ── Main render ──

 return (
 <div className="space-y-6">
 {/* Header */}
 <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
 <div>
 <h1 className="text-2xl font-bold tracking-tight flex items-center gap-2">
 <Users className="w-6 h-6" />
 STATION STAFFING
 </h1>
 {roster && (
 <div className="flex items-center gap-3 mt-1">
 {roster.is_locked && (
 <Badge variant="secondary" className="bg-amber-100 text-amber-800">
 <Lock className="w-3 h-3 mr-1" /> Locked
 </Badge>
 )}
 </div>
 )}
 </div>
 {/* Duty Chief — boxed card, top right */}
 {roster && (
 <Card className="min-w-[200px] border-amber-200 bg-amber-50">
 <CardContent className="py-3 px-4">
 <div className="flex items-center gap-2 text-xs font-semibold text-amber-700 uppercase tracking-wide mb-1">
 <Shield className="w-4 h-4" />
 Duty Chief
 </div>
 {editMode ? (
 <select
 value={editDutyChiefId ?? ''}
 onChange={(e) => setEditDutyChiefId(e.target.value || null)}
 className="w-full rounded-md border border-amber-300 bg-white px-2 py-1.5 text-sm font-semibold"
 >
 <option value="">— Select —</option>
 {allEmployees
 .filter((e) => ['Administration', 'Chief', 'Division Chief', 'Battalion Chief'].includes(e.rank))
 .map((e) => (
 <option key={e.id} value={e.id}>{e.name} ({e.rank})</option>
 ))}
 </select>
 ) : (
 <p className="text-base font-bold text-gray-900">
 {roster.duty_chief_name || 'Not assigned'}
 </p>
 )}
 </CardContent>
 </Card>
 )}
 </div>
 <div className="flex items-center gap-2 flex-wrap">
 <Button variant="outline" size="sm" onClick={() => setCalendarView((v) => !v)}>
 <CalendarDays className="w-4 h-4 mr-1" />
 {calendarView ? 'Daily View' : 'Calendar'}
 </Button>
 <Button variant="outline" size="sm" onClick={() => setShowGenerate((v) => !v)}>
 <RefreshCw className="w-4 h-4 mr-1" />
 Generate
 </Button>
 {roster && !roster.is_locked && !editMode && (
 <>
 <Button
 variant="outline"
 size="sm"
 onClick={enterEditMode}
 >
 <Pencil className="w-4 h-4 mr-1" />
 Edit
 </Button>
 <Button
 variant="outline"
 size="sm"
 onClick={handleLock}
 disabled={locking}
 >
 {locking ? <Loader2 className="w-4 h-4 animate-spin mr-1" /> : <Lock className="w-4 h-4 mr-1" />}
 Lock Roster
 </Button>
 </>
 )}
 {editMode && (
 <>
 <Button
 size="sm"
 onClick={handleSaveAssignments}
 disabled={saving}
 >
 {saving ? <Loader2 className="w-4 h-4 animate-spin mr-1" /> : <CheckCircle2 className="w-4 h-4 mr-1" />}
 Save
 </Button>
 <Button
 variant="outline"
 size="sm"
 onClick={exitEditMode}
 >
 Cancel
 </Button>
 </>
 )}
 {roster?.is_locked && !editMode && (
 <Button
 variant="outline"
 size="sm"
 onClick={handleUnlock}
 disabled={locking}
 >
 {locking ? <Loader2 className="w-4 h-4 animate-spin mr-1" /> : <Unlock className="w-4 h-4 mr-1" />}
 Unlock
 </Button>
 )}
 </div>

 {/* Date Navigation + Shift Selector */}
 {!calendarView && (
 <div className="flex items-center gap-2 flex-wrap">
 <Button variant="ghost" size="sm" onClick={goPrev}>
 <ChevronLeft className="w-4 h-4" />
 </Button>
 <input
 type="date"
 value={formatDate(selectedDate)}
 onChange={(e) => {
 const d = new Date(e.target.value + 'T00:00:00')
 if (!isNaN(d.getTime())) setSelectedDate(d)
 }}
 className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm"
 />
 <Button variant="ghost" size="sm" onClick={goNext}>
 <ChevronRight className="w-4 h-4" />
 </Button>
 {/* Shift dropdown — filters which shift's personnel to view on this date */}
 <select
 value={viewShift}
 onChange={(e) => setViewShift(e.target.value)}
 className={`rounded-md border px-3 py-1.5 text-sm font-bold cursor-pointer ${
 SHIFT_COLORS[viewShift] ? `${SHIFT_COLORS[viewShift].bg} ${SHIFT_COLORS[viewShift].text} ${SHIFT_COLORS[viewShift].border}` : 'border-gray-300 bg-white'
 }`}
 >
 <option value="A">A Shift</option>
 <option value="B">B Shift</option>
 <option value="C">C Shift</option>
 </select>
 <Button variant="outline" size="sm" onClick={goToday}>
 <Calendar className="w-4 h-4 mr-1" />
 Today
 </Button>
 <span className="text-sm text-gray-500 ml-2">
 {formatDisplayDate(selectedDate)}
 </span>
 </div>
 )}

 {/* Error */}
 {error && (
 <div className="rounded-lg border border-red-300 bg-red-50 p-4 text-red-700 text-sm flex items-start gap-2">
 <AlertTriangle className="w-4 h-4 mt-0.5 flex-shrink-0" />
 <span>{error}</span>
 </div>
 )}

 {/* Generate Roster Panel */}
 {showGenerate && (
 <Card>
 <CardHeader className="pb-2">
 <CardTitle className="text-base">Generate Roster</CardTitle>
 </CardHeader>
 <CardContent className="space-y-3">
 <div className="flex items-center gap-3 flex-wrap">
 <label className="text-sm">
 Start:
 <input
 type="date"
 value={genStart}
 onChange={(e) => setGenStart(e.target.value)}
 className="ml-2 rounded-md border border-gray-300 bg-white px-2 py-1 text-sm"
 />
 </label>
 <label className="text-sm">
 End:
 <input
 type="date"
 value={genEnd}
 onChange={(e) => setGenEnd(e.target.value)}
 className="ml-2 rounded-md border border-gray-300 bg-white px-2 py-1 text-sm"
 />
 </label>
 <Button size="sm" onClick={handleGenerate} disabled={generating}>
 {generating ? <Loader2 className="w-4 h-4 animate-spin mr-1" /> : <RefreshCw className="w-4 h-4 mr-1" />}
 Generate
 </Button>
 </div>
 {genResult && (
 <div className="rounded-md border p-3 text-sm space-y-1">
 <div className="flex items-center gap-2">
 <CheckCircle2 className="w-4 h-4 text-green-600" />
 <span>Created: <strong>{genResult.created}</strong></span>
 <span className="text-gray-500">Skipped: {genResult.skipped}</span>
 </div>
 {genResult.alerts.length > 0 && (
 <div className="text-amber-600">
 {genResult.alerts.map((a, i) => (
 <div key={i} className="flex items-start gap-1">
 <AlertTriangle className="w-3 h-3 mt-0.5 flex-shrink-0" />
 <span>{a}</span>
 </div>
 ))}
 </div>
 )}
 </div>
 )}
 </CardContent>
 </Card>
 )}

 {/* Calendar View */}
 {calendarView && renderCalendar()}

 {/* Daily Detail View */}
 {!calendarView && (
 <>
 {loading ? (
 <div className="flex items-center justify-center h-64">
 <Loader2 className="w-6 h-6 animate-spin text-indigo-500" />
 </div>
 ) : !roster ? (
 <Card>
 <CardContent className="py-12 text-center text-gray-500">
 No roster data for this date. Use the Generate button to create one.
 </CardContent>
 </Card>
 ) : (
 <>
 {/* Station Grid */}
 <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
 {STATION_NAMES.map((station) => {
 const personnel = stationAssignments[station] ?? []
 const stationObj = allStations.find((s) => s.name === station)
 return (
 <Card key={station} className="relative">
 {editMode && editedStations.has(station) && (
 <div className="absolute top-2 right-2 w-3 h-3 rounded-full bg-green-500 border-2 border-white" title="Modified" />
 )}
 <CardHeader className="pb-2">
 <CardTitle className="text-sm font-semibold">{station}</CardTitle>
 </CardHeader>
 <CardContent className="space-y-2">
 {personnel.length === 0 ? (
 <p className="text-xs text-gray-400">No personnel assigned</p>
 ) : (
 personnel.map((p, i) => (
 <div
 key={i}
 className="flex items-center justify-between gap-1 py-1 border-b border-gray-100 last:border-0"
 >
 <span className="text-sm font-medium truncate">{p.employee_name}</span>
 <div className="flex items-center gap-1 flex-shrink-0">
 {editMode ? (
 <>
 <select
 value={p.assignment_type}
 onChange={(e) => setAssignmentType(p.employee_id!, e.target.value)}
 className="text-[10px] rounded border border-gray-300 bg-white px-1 py-0.5"
 >
 <option value="regular">Regular</option>
 <option value="overtime">OT</option>
 <option value="trade_in">Trade In</option>
 <option value="callback">Callback</option>
 </select>
 <select
 value={p.station_id ?? ''}
 onChange={(e) => moveToStation(p.employee_id!, e.target.value)}
 className="text-[10px] rounded border border-gray-300 bg-white px-1 py-0.5"
 >
 {allStations.map((s) => (
 <option key={s.id} value={s.id}>{s.name}</option>
 ))}
 </select>
 <button
 onClick={() => toggle24hr(p.employee_id!)}
 className={`text-[10px] px-1 py-0.5 rounded ${p.is_24hr ? 'bg-orange-600 text-white' : 'bg-gray-200 text-gray-600'}`}
 title="Toggle 24-hour shift"
 >
 24hr
 </button>
 <button
 onClick={() => toggleCism(p.employee_id!)}
 className={`text-[10px] px-1 py-0.5 rounded ${p.is_cism_coverage ? 'bg-teal-600 text-white' : 'bg-gray-200 text-gray-600'}`}
 title="Toggle CISM"
 >
 CISM
 </button>
 <button
 onClick={() => removeAssignment(p.employee_id!)}
 className="text-red-500 hover:text-red-700 text-xs font-bold px-1"
 title="Remove"
 >
 ✕
 </button>
 </>
 ) : (
 <div className="flex items-center gap-1">
 {p.is_24hr && (
 <Badge className="text-[10px] px-1 py-0 bg-orange-600 text-white">24hr</Badge>
 )}
 <Badge
 className={`text-[10px] px-1.5 py-0 ${rankBadgeClass(p.employee_rank, p.assignment_type)}`}
 >
 {p.employee_rank}
 {(p.assignment_type.toLowerCase() === 'ot' || p.assignment_type.toLowerCase() === 'overtime') && ' (OT)'}
 {p.assignment_type.toLowerCase() === 'reserve' && ' (Res)'}
 </Badge>
 </div>
 )}
 </div>
 </div>
 ))
 )}
 {/* Add personnel button in edit mode */}
 {editMode && stationObj && (
 <div className="pt-1">
 <select
 value=""
 onChange={(e) => {
 if (e.target.value) addAssignment(e.target.value, stationObj.id)
 }}
 className="w-full text-xs rounded border border-dashed border-gray-300 bg-white px-2 py-1 text-gray-500"
 >
 <option value="">+ Add personnel...</option>
 {/* Group by rank for clarity */}
 {(['Captain', 'Lieutenant', 'Engineer', 'Firefighter'] as const).map((rank) => {
 const emps = unassignedEmployees.filter((e) => e.rank === rank)
 return emps.length > 0 ? (
 <optgroup key={rank} label={`${rank}s`}>
 {emps.map((e) => (
 <option key={e.id} value={e.id}>{e.name}{e.shift ? ` (${e.shift} Shift)` : ''}</option>
 ))}
 </optgroup>
 ) : null
 })}
 {/* Reserve Firefighters */}
 {unassignedEmployees.filter((e) => e.rank === 'Reserve').length > 0 && (
 <optgroup label="Reserve Firefighters">
 {unassignedEmployees.filter((e) => e.rank === 'Reserve').map((e) => (
 <option key={e.id} value={e.id}>{e.name} (Reserve)</option>
 ))}
 </optgroup>
 )}
 {/* Other ranks */}
 {unassignedEmployees.filter((e) => !['Captain','Lieutenant','Engineer','Firefighter','Reserve'].includes(e.rank)).length > 0 && (
 <optgroup label="Other">
 {unassignedEmployees.filter((e) => !['Captain','Lieutenant','Engineer','Firefighter','Reserve'].includes(e.rank)).map((e) => (
 <option key={e.id} value={e.id}>{e.name} ({e.rank})</option>
 ))}
 </optgroup>
 )}
 </select>
 </div>
 )}
 </CardContent>
 </Card>
 )
 })}

 {/* CISM Coverage */}
 <Card>
 <CardHeader className="pb-2">
 <CardTitle className="text-sm font-semibold">CISM Coverage</CardTitle>
 </CardHeader>
 <CardContent className="space-y-2">
 {cismPersonnel.length === 0 ? (
 <p className="text-xs text-gray-400">No CISM coverage</p>
 ) : (
 cismPersonnel.map((p, i) => (
 <div
 key={i}
 className="flex items-center justify-between py-1 border-b border-gray-100 last:border-0"
 >
 <span className="text-sm font-medium">{p.employee_name}</span>
 <Badge className="text-[10px] px-1.5 py-0 bg-teal-600 text-white">
 {p.station_name}
 </Badge>
 </div>
 ))
 )}
 {editMode && (
 <p className="text-[10px] text-gray-400 italic">Toggle CISM on personnel above</p>
 )}
 </CardContent>
 </Card>
 </div>

 {/* Absence Section */}
 <Card>
 <CardHeader className="pb-2 flex flex-row items-center justify-between">
 <CardTitle className="text-sm font-semibold">Absences</CardTitle>
 {!roster.is_locked && (
 <Button variant="outline" size="sm" onClick={() => { if (!editMode) loadEmployeesIfNeeded(); setShowAddAbsence((v) => !v) }}>
 {showAddAbsence ? 'Cancel' : '+ Add Absence'}
 </Button>
 )}
 </CardHeader>
 <CardContent>
 {/* Add absence form */}
 {showAddAbsence && (
 <div className="flex items-center gap-2 mb-3 pb-3 border-b border-gray-200">
 <select
 value={newAbsenceEmpId}
 onChange={(e) => setNewAbsenceEmpId(e.target.value)}
 className="text-sm rounded border border-gray-300 bg-white px-2 py-1 flex-1"
 >
 <option value="">Select employee...</option>
 {allEmployees.map((e) => (
 <option key={e.id} value={e.id}>{e.name}</option>
 ))}
 </select>
 <select
 value={newAbsenceType}
 onChange={(e) => setNewAbsenceType(e.target.value)}
 className="text-sm rounded border border-gray-300 bg-white px-2 py-1"
 >
 {ABSENCE_TYPES.map((t) => (
 <option key={t} value={t}>{t}</option>
 ))}
 </select>
 <Button size="sm" onClick={handleAddAbsence} disabled={!newAbsenceEmpId}>
 Add
 </Button>
 </div>
 )}

 {roster.absences.length === 0 && !showAddAbsence ? (
 <p className="text-xs text-gray-400">No absences recorded</p>
 ) : (
 <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-3">
 {ABSENCE_TYPES.map((type) => {
 const absencesOfType = roster.absences.filter((a) => a.absence_type === type)
 return (
 <div key={type} className="space-y-1">
 <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide">
 {type}
 </div>
 {absencesOfType.length === 0 ? (
 <div className="text-xs text-gray-400">&mdash;</div>
 ) : (
 absencesOfType.map((a, i) => (
 <div key={i} className="text-sm flex items-center justify-between gap-1">
 <span>{a.employee_name}</span>
 {editMode && a.id && (
 <button
 onClick={() => handleDeleteAbsence(a.id!)}
 className="text-red-500 hover:text-red-700 text-xs font-bold"
 title="Remove absence"
 >
 ✕
 </button>
 )}
 </div>
 ))
 )}
 </div>
 )
 })}
 </div>
 )}
 </CardContent>
 </Card>
 </>
 )}
 </>
 )}
 </div>
 )
}
