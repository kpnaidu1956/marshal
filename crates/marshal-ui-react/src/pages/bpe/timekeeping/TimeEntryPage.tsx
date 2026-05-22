import { useState, useEffect, useCallback, useMemo } from 'react'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import {
 Loader2,
 RefreshCw,
 ChevronLeft,
 ChevronRight,
 CheckCircle2,
 Clock,
 X,
 Send,
 Plus,
} from 'lucide-react'

/* ------------------------------------------------------------------ */
/* Types */
/* ------------------------------------------------------------------ */

interface PayCode {
 id: string
 code: string
 display_name: string
 category: string
 is_overtime: boolean
}

interface TimeEntry {
 id: string
 employee_id: string
 pay_code_id: string
 pay_code?: string
 pay_code_name?: string
 work_date: string
 start_time: string | null
 hours: number
 notes: string | null
 status: 'draft' | 'submitted' | 'approved' | 'rejected'
}

interface PayPeriod {
 id: string
 period_start: string
 period_end: string
 status?: string
}

interface Employee {
 id: string
 first_name: string
 last_name: string
 rank: string
 shift_assignment?: string
}

/* ------------------------------------------------------------------ */
/* Helpers */
/* ------------------------------------------------------------------ */

function fmtDate(d: Date): string {
 return d.toISOString().slice(0, 10)
}

function parseDate(s: string): Date {
 const [y, m, d] = s.split('-').map(Number)
 return new Date(y, m - 1, d)
}

function dayOfWeekShort(d: Date): string {
 return d.toLocaleDateString('en-US', { weekday: 'short' })
}

function lastDayOfMonth(year: number, month: number): number {
 return new Date(year, month + 1, 0).getDate()
}

/** Compute the two semi-monthly pay periods for a given month */
function payPeriodsForMonth(year: number, month: number): { start: string; end: string; label: string }[] {
 const m = String(month + 1).padStart(2, '0')
 const lastDay = lastDayOfMonth(year, month)
 return [
 {
 start: `${year}-${m}-01`,
 end: `${year}-${m}-15`,
 label: `${m}/1/${year} – ${m}/15/${year}`,
 },
 {
 start: `${year}-${m}-16`,
 end: `${year}-${m}-${lastDay}`,
 label: `${m}/16/${year} – ${m}/${lastDay}/${year}`,
 },
 ]
}

/** Generate array of dates between start and end (inclusive) */
function dateRange(startStr: string, endStr: string): Date[] {
 const dates: Date[] = []
 const cur = parseDate(startStr)
 const end = parseDate(endStr)
 while (cur <= end) {
 dates.push(new Date(cur))
 cur.setDate(cur.getDate() + 1)
 }
 return dates
}

function isWeekend(d: Date): boolean {
 const day = d.getDay()
 return day === 0 || day === 6
}

/* ------------------------------------------------------------------ */
/* Component */
/* ------------------------------------------------------------------ */

export function TimeEntryPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const today = new Date()
 const [viewYear, setViewYear] = useState(today.getFullYear())
 const [viewMonth, setViewMonth] = useState(today.getMonth())

 // Which half: 0 = 1st-15th, 1 = 16th-end
 const todayHalf = today.getDate() <= 15 ? 0 : 1
 const [periodHalf, setPeriodHalf] = useState(todayHalf)

 const [entries, setEntries] = useState<TimeEntry[]>([])
 const [payCodes, setPayCodes] = useState<PayCode[]>([])
 const [periods, setPeriods] = useState<PayPeriod[]>([])
 const [employees, setEmployees] = useState<Employee[]>([])
 const [selectedEmpId, setSelectedEmpId] = useState<string>('')
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Entry form (inline cell edit)
 const [editingCell, setEditingCell] = useState<{ date: string; payCodeId: string } | null>(null)
 const [cellHours, setCellHours] = useState(0)
 const [cellSaving, setCellSaving] = useState(false)

 // Enter Hour dialog
 const [showEnterHour, setShowEnterHour] = useState(false)
 const [ehDate, setEhDate] = useState('')
 const [ehHours, setEhHours] = useState(8)
 const [ehStart, setEhStart] = useState('08:00')
 const [ehPayCode, setEhPayCode] = useState('')
 const [ehNotes, setEhNotes] = useState('')
 const [ehSaving, setEhSaving] = useState(false)

 // Certification
 const [showCertify, setShowCertify] = useState(false)
 const [certifyChecked, setCertifyChecked] = useState(false)
 const [certifying, setCertifying] = useState(false)

 const client = useMemo(() => (token ? new BpeClient(token) : null), [token])

 // ── Computed pay period ──

 const ppOptions = useMemo(() => payPeriodsForMonth(viewYear, viewMonth), [viewYear, viewMonth])
 const currentPP = ppOptions[periodHalf] ?? ppOptions[0]
 const periodDates = useMemo(() => dateRange(currentPP.start, currentPP.end), [currentPP])

 // ── Fetch data ──

 const fetchData = useCallback(async () => {
 if (!client || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const empId = selectedEmpId || undefined
 const [pcRes, entRes, perRes, empRes] = await Promise.all([
 client.tkListPayCodes(orgSlug),
 client.tkListTimeEntries(orgSlug, {
 start: currentPP.start,
 end: currentPP.end,
 ...(empId ? { employee_id: empId } : {}),
 }),
 client.tkListPeriods(orgSlug).catch(() => ({ data: [] })),
 client.tkListEmployees(orgSlug, { status: 'active', per_page: '200' }),
 ])

 setPayCodes((pcRes.data as PayCode[]).filter((p) => p.category === 'work' || p.category === 'leave'))
 setEntries(entRes.data as TimeEntry[])
 setPeriods(perRes.data as PayPeriod[])
 const emps = empRes.data as Employee[]
 setEmployees(emps)
 if (!selectedEmpId && emps.length > 0) {
 setSelectedEmpId(emps[0].id)
 }
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load timecard data')
 } finally {
 setLoading(false)
 }
 }, [client, orgSlug, currentPP.start, currentPP.end, selectedEmpId])

 useEffect(() => { fetchData() }, [fetchData])

 // ── Lookups ──

 const payCodeMap = useMemo(() => {
 const m = new Map<string, PayCode>()
 for (const pc of payCodes) m.set(pc.id, pc)
 return m
 }, [payCodes])

 // Only show pay codes that have hours or are common work codes
 const activePayCodes = useMemo(() => {
 const usedIds = new Set(entries.map((e) => e.pay_code_id))
 // Always show work pay codes, plus any leave codes that have entries
 return payCodes.filter((pc) => pc.category === 'work' || usedIds.has(pc.id))
 }, [payCodes, entries])

 // entry lookup: date -> payCodeId -> TimeEntry
 const entryMap = useMemo(() => {
 const m = new Map<string, Map<string, TimeEntry>>()
 for (const e of entries) {
 if (!m.has(e.work_date)) m.set(e.work_date, new Map())
 m.get(e.work_date)!.set(e.pay_code_id, e)
 }
 return m
 }, [entries])

 // total hours per date
 const dateTotals = useMemo(() => {
 const m = new Map<string, number>()
 for (const e of entries) {
 m.set(e.work_date, (m.get(e.work_date) ?? 0) + e.hours)
 }
 return m
 }, [entries])

 // total hours per pay code
 const payCodeTotals = useMemo(() => {
 const m = new Map<string, number>()
 for (const e of entries) {
 m.set(e.pay_code_id, (m.get(e.pay_code_id) ?? 0) + e.hours)
 }
 return m
 }, [entries])

 // Grand total
 const grandTotalPaid = useMemo(() => entries.reduce((s, e) => s + e.hours, 0), [entries])

 const selectedEmployee = employees.find((e) => e.id === selectedEmpId)

 // ── Cell click → inline edit ──

 function handleCellClick(dateStr: string, pcId: string) {
 const existing = entryMap.get(dateStr)?.get(pcId)
 if (existing && existing.status !== 'draft') return // can't edit non-draft
 setEditingCell({ date: dateStr, payCodeId: pcId })
 setCellHours(existing?.hours ?? 0)
 }

 async function saveCellEdit() {
 if (!client || !orgSlug || !editingCell) return
 setCellSaving(true)
 try {
 const existing = entryMap.get(editingCell.date)?.get(editingCell.payCodeId)
 if (cellHours <= 0 && existing) {
 // Delete if zeroed out
 await client.tkDeleteTimeEntry(existing.id)
 } else if (cellHours > 0 && existing) {
 // Update
 await client.tkUpdateTimeEntry(existing.id, { hours: cellHours })
 } else if (cellHours > 0) {
 // Create
 await client.tkCreateTimeEntry({
 organization_id: orgSlug,
 employee_id: selectedEmpId,
 pay_code_id: editingCell.payCodeId,
 work_date: editingCell.date,
 start_time: '08:00',
 hours: cellHours,
 })
 }
 setEditingCell(null)
 await fetchData()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to save')
 } finally {
 setCellSaving(false)
 }
 }

 // ── Enter Hour dialog ──

 function openEnterHour(dateStr?: string) {
 setEhDate(dateStr ?? fmtDate(today))
 setEhHours(8)
 setEhStart('08:00')
 setEhPayCode(payCodes.find((p) => p.code === 'REG')?.id ?? payCodes[0]?.id ?? '')
 setEhNotes('')
 setShowEnterHour(true)
 }

 async function handleEnterHour() {
 if (!client || !orgSlug || !ehPayCode) return
 setEhSaving(true)
 try {
 await client.tkCreateTimeEntry({
 organization_id: orgSlug,
 employee_id: selectedEmpId,
 pay_code_id: ehPayCode,
 work_date: ehDate,
 start_time: ehStart || null,
 hours: ehHours,
 notes: ehNotes || null,
 })
 setShowEnterHour(false)
 await fetchData()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to create entry')
 } finally {
 setEhSaving(false)
 }
 }

 // ── Submit all draft ──

 async function handleSubmitAll() {
 if (!client) return
 const drafts = entries.filter((e) => e.status === 'draft')
 for (const d of drafts) {
 await client.tkSubmitTimeEntry(d.id).catch(() => {})
 }
 await fetchData()
 }

 // ── Certify ──

 async function handleCertify() {
 if (!client || !orgSlug) return
 setCertifying(true)
 try {
 // Find matching period or pass null
 const matchingPeriod = periods.find(
 (p) => p.period_start <= currentPP.end && p.period_end >= currentPP.start,
 )
 // If no matching period, create one for the viewed pay period first
 let periodId = matchingPeriod?.id ?? null
 if (!periodId) {
 try {
 const created = await client.tkCreatePeriod({
 organization_id: orgSlug,
 period_start: currentPP.start,
 period_end: currentPP.end,
 })
 periodId = (created.data as { id: string })?.id ?? null
 } catch { /* will pass null, backend auto-creates */ }
 }
 await client.tkCertifyTimecard({
 organization_id: orgSlug,
 employee_id: selectedEmpId,
 period_id: periodId,
 signature_text: 'I certify that the above time card is correct.',
 })
 setShowCertify(false)
 setCertifyChecked(false)
 await fetchData()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to certify')
 } finally {
 setCertifying(false)
 }
 }

 // ── Navigation ──

 function prevPeriod() {
 if (periodHalf === 1) {
 setPeriodHalf(0)
 } else {
 setPeriodHalf(1)
 if (viewMonth === 0) { setViewMonth(11); setViewYear(viewYear - 1) }
 else setViewMonth(viewMonth - 1)
 }
 }

 function nextPeriod() {
 if (periodHalf === 0) {
 setPeriodHalf(1)
 } else {
 setPeriodHalf(0)
 if (viewMonth === 11) { setViewMonth(0); setViewYear(viewYear + 1) }
 else setViewMonth(viewMonth + 1)
 }
 }

 // ── Guards ──

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization.</p></div>
 }

 if (loading && entries.length === 0) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 const draftCount = entries.filter((e) => e.status === 'draft').length

 // ── Render ──

 return (
 <div className="space-y-4">
 {/* ── Header ── */}
 <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
 <div>
 <h1 className="text-2xl font-bold tracking-tight flex items-center gap-2">
 <Clock className="w-6 h-6" />
 Time Card
 </h1>
 <p className="text-xs text-gray-500 mt-0.5">Current Status: N/A</p>
 </div>
 <div className="flex items-center gap-2 flex-wrap text-sm">
 <Button variant="outline" size="sm" onClick={() => openEnterHour()}>
 <Plus className="w-4 h-4 mr-1" /> Enter Hour
 </Button>
 {draftCount > 0 && (
 <Button size="sm" onClick={handleSubmitAll}>
 <Send className="w-4 h-4 mr-1" /> Submit All ({draftCount})
 </Button>
 )}
 <Button variant="ghost" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4" />
 </Button>
 </div>
 </div>

 {/* ── Employee Selector ── */}
 <div className="flex flex-wrap items-center gap-3">
 <label className="text-sm font-medium">Employee:</label>
 <select
 value={selectedEmpId}
 onChange={(e) => setSelectedEmpId(e.target.value)}
 className="rounded-md border border-gray-300 bg-white px-2 py-1 text-sm max-w-xs"
 >
 {employees.map((e) => (
 <option key={e.id} value={e.id}>{e.last_name}, {e.first_name}</option>
 ))}
 </select>
 {selectedEmployee && (
 <>
 <Badge variant="secondary">{selectedEmployee.rank}</Badge>
 {selectedEmployee.shift_assignment && (
 <Badge variant="outline">{selectedEmployee.shift_assignment} Shift</Badge>
 )}
 </>
 )}
 <span className="text-sm text-gray-500">Status: <span className="font-medium text-green-600">Active</span></span>
 </div>

 {/* ── Error ── */}
 {error && (
 <div className="rounded-lg border border-red-300 bg-red-50 p-3 text-red-700 text-sm flex justify-between">
 <span>{error}</span>
 <button onClick={() => setError(null)}><X className="w-4 h-4" /></button>
 </div>
 )}

 {/* ── Pay Period Navigation ── */}
 <div className="flex items-center gap-2">
 <Button variant="ghost" size="sm" onClick={prevPeriod}><ChevronLeft className="w-4 h-4" /></Button>
 <span className="text-sm font-semibold px-2">
 {parseDate(currentPP.start).toLocaleDateString('en-US', { weekday: 'long', month: 'numeric', day: 'numeric', year: 'numeric' })}
 {' to '}
 {parseDate(currentPP.end).toLocaleDateString('en-US', { weekday: 'long', month: 'numeric', day: 'numeric', year: 'numeric' })}
 </span>
 <Button variant="ghost" size="sm" onClick={nextPeriod}><ChevronRight className="w-4 h-4" /></Button>
 </div>

 {/* ── Timecard Grid ── */}
 <Card>
 <CardContent className="p-0 overflow-x-auto">
 <table className="w-full text-xs border-collapse">
 <thead>
 {/* Day-of-week row */}
 <tr className="border-b border-gray-200">
 <th className="text-left px-2 py-1.5 font-medium text-gray-500 min-w-[100px] sticky left-0 bg-white z-10"></th>
 {periodDates.map((d) => {
 const wkend = isWeekend(d)
 return (
 <th
 key={fmtDate(d)}
 className={`text-center px-1 py-1.5 font-medium min-w-[52px] ${wkend ? 'text-gray-400 bg-gray-50' : 'text-gray-600'}`}
 >
 <div>{dayOfWeekShort(d)}</div>
 <div className="font-bold text-sm">{d.getDate()}</div>
 </th>
 )
 })}
 <th className="text-center px-2 py-1.5 font-medium text-gray-600 min-w-[60px]">Total Paid</th>
 <th className="text-center px-2 py-1.5 font-medium text-gray-600 min-w-[60px]">Total Uni</th>
 </tr>
 </thead>
 <tbody>
 {/* ── Total Hours row ── */}
 <tr className="border-b border-gray-300 bg-gray-50">
 <td className="px-2 py-1.5 font-semibold text-gray-700 sticky left-0 bg-gray-50 z-10">Total Hours</td>
 {periodDates.map((d) => {
 const dateStr = fmtDate(d)
 const total = dateTotals.get(dateStr) ?? 0
 const wkend = isWeekend(d)
 return (
 <td
 key={dateStr}
 className={`text-center py-1.5 font-semibold cursor-pointer hover:bg-blue-50 ${
 wkend ? 'text-gray-400 bg-gray-50' : total > 0 ? 'text-blue-600' : 'text-gray-400'
 }`}
 onClick={() => openEnterHour(dateStr)}
 >
 {total > 0 ? total.toFixed(2) : wkend ? '0.00' : ''}
 </td>
 )
 })}
 <td className="text-center py-1.5 font-bold text-blue-700 bg-gray-50">
 {grandTotalPaid > 0 ? grandTotalPaid.toFixed(2) : ''}
 </td>
 <td className="text-center py-1.5 font-bold text-gray-500 bg-gray-50">0.00</td>
 </tr>

 {/* ── Pay Codes label ── */}
 <tr className="border-b border-gray-200">
 <td colSpan={periodDates.length + 3} className="px-2 py-1 text-[10px] text-gray-400 italic">Pay Codes</td>
 </tr>

 {/* ── Pay Code rows ── */}
 {activePayCodes.map((pc) => {
 const pcTotal = payCodeTotals.get(pc.id) ?? 0
 return (
 <tr key={pc.id} className="border-b border-gray-100 hover:bg-gray-50">
 <td className="px-2 py-1 font-medium text-gray-700 sticky left-0 bg-white z-10">
 {pc.code}
 </td>
 {periodDates.map((d) => {
 const dateStr = fmtDate(d)
 const entry = entryMap.get(dateStr)?.get(pc.id)
 const hrs = entry?.hours ?? 0
 const wkend = isWeekend(d)
 const isEditing = editingCell?.date === dateStr && editingCell?.payCodeId === pc.id

 if (isEditing) {
 return (
 <td key={dateStr} className="text-center p-0">
 <input
 type="number"
 value={cellHours}
 onChange={(e) => setCellHours(parseFloat(e.target.value) || 0)}
 onBlur={saveCellEdit}
 onKeyDown={(e) => { if (e.key === 'Enter') saveCellEdit(); if (e.key === 'Escape') setEditingCell(null) }}
 autoFocus
 min={0}
 max={48}
 step={0.5}
 className="w-full text-center text-xs py-1 border-2 border-blue-500 bg-blue-50 outline-none"
 disabled={cellSaving}
 />
 </td>
 )
 }

 return (
 <td
 key={dateStr}
 className={`text-center py-1 cursor-pointer hover:bg-blue-50 ${
 wkend ? 'bg-gray-50' : ''
 } ${hrs > 0 ? 'text-gray-900 font-medium' : 'text-gray-300'} ${
 entry?.status === 'approved' ? 'text-green-700' : ''
 } ${entry?.status === 'submitted' ? 'text-amber-600' : ''}`}
 onClick={() => handleCellClick(dateStr, pc.id)}
 title={entry ? `${entry.status} · ${hrs}h` : 'Click to add'}
 >
 {hrs > 0 ? hrs.toFixed(2) : ''}
 </td>
 )
 })}
 <td className="text-center py-1 font-semibold text-gray-800">{pcTotal > 0 ? pcTotal.toFixed(2) : ''}</td>
 <td className="text-center py-1 font-semibold text-gray-800">{pcTotal > 0 ? pcTotal.toFixed(2) : ''}</td>
 </tr>
 )
 })}

 {/* ── Grand Totals ── */}
 <tr className="border-t-2 border-gray-400 bg-gray-100">
 <td className="px-2 py-2 font-bold text-gray-900 sticky left-0 bg-gray-100 z-10">Grand Totals</td>
 {periodDates.map((d) => <td key={fmtDate(d)} />)}
 <td className="text-center py-2 font-bold text-lg text-gray-900">{grandTotalPaid.toFixed(2)}</td>
 <td className="text-center py-2 font-bold text-lg text-gray-500">0.00</td>
 </tr>

 {/* ── Schedule (same table for column alignment) ── */}
 <tr className="border-t-2 border-gray-300">
 <td colSpan={periodDates.length + 3} className="px-2 py-1 text-[10px] text-gray-400 italic sticky left-0 bg-white z-10">Schedule</td>
 </tr>
 <tr className="border-b border-gray-100">
 <td className="px-2 py-1 font-medium text-gray-600 sticky left-0 bg-white z-10">Start</td>
 {periodDates.map((d) => (
 <td key={fmtDate(d)} className={`text-center py-1 ${isWeekend(d) ? 'text-gray-300' : 'text-gray-600'}`}>
 {isWeekend(d) ? '' : '08:00 AM'}
 </td>
 ))}
 <td /><td />
 </tr>
 <tr className="border-b border-gray-100">
 <td className="px-2 py-1 font-medium text-gray-600 sticky left-0 bg-white z-10">End</td>
 {periodDates.map((d) => (
 <td key={fmtDate(d)} className={`text-center py-1 ${isWeekend(d) ? 'text-gray-300' : 'text-gray-600'}`}>
 {isWeekend(d) ? '' : '04:00 PM'}
 </td>
 ))}
 <td /><td />
 </tr>
 <tr className="border-b border-gray-200">
 <td className="px-2 py-1 font-semibold text-gray-700 sticky left-0 bg-white z-10">Total Scheduled</td>
 {periodDates.map((d) => (
 <td key={fmtDate(d)} className={`text-center py-1 font-semibold ${isWeekend(d) ? 'text-gray-300' : 'text-gray-700'}`}>
 {isWeekend(d) ? '0.00' : '8.00'}
 </td>
 ))}
 <td /><td />
 </tr>
 </tbody>
 </table>
 </CardContent>
 </Card>

 {/* ── Verification / Certification ── */}
 <Card>
 <CardHeader className="pb-1"><CardTitle className="text-sm">Verification</CardTitle></CardHeader>
 <CardContent>
 {!showCertify ? (
 <div className="flex items-center justify-between">
 <div className="text-sm text-gray-600">
 <p>I certify that the above time card is correct.</p>
 <p className="text-xs text-gray-400 mt-1">
 Period: {currentPP.start} — {currentPP.end}
 </p>
 </div>
 <Button onClick={() => setShowCertify(true)} variant="outline" size="sm">
 <CheckCircle2 className="w-4 h-4 mr-1" /> Certify Timecard
 </Button>
 </div>
 ) : (
 <div className="space-y-3">
 <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-3">
 <label className="flex items-start gap-2 cursor-pointer">
 <input
 type="checkbox"
 checked={certifyChecked}
 onChange={(e) => setCertifyChecked(e.target.checked)}
 className="mt-0.5 rounded border-gray-300"
 />
 <span className="text-sm text-gray-700">
 I certify that the above time card is correct.
 </span>
 </label>
 <p className="text-xs text-gray-500 mt-2">
 Period: {currentPP.start} — {currentPP.end} &nbsp;|&nbsp; Employee: {selectedEmployee ? `${selectedEmployee.last_name}, ${selectedEmployee.first_name}` : '—'}
 </p>
 </div>
 <div className="flex justify-end gap-2">
 <Button variant="outline" size="sm" onClick={() => { setShowCertify(false); setCertifyChecked(false) }}>Cancel</Button>
 <Button size="sm" onClick={handleCertify} disabled={!certifyChecked || certifying}>
 {certifying ? <Loader2 className="w-4 h-4 animate-spin mr-1" /> : <CheckCircle2 className="w-4 h-4 mr-1" />}
 Certify & Submit
 </Button>
 </div>
 </div>
 )}
 </CardContent>
 </Card>

 {/* ── Enter Hour Dialog (modal-like overlay) ── */}
 {showEnterHour && (
 <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setShowEnterHour(false)}>
 <Card className="w-full max-w-md mx-4" onClick={(e) => e.stopPropagation()}>
 <CardHeader className="pb-2">
 <div className="flex items-center justify-between">
 <CardTitle className="text-base">Enter Hour</CardTitle>
 <Button variant="ghost" size="sm" onClick={() => setShowEnterHour(false)}><X className="w-4 h-4" /></Button>
 </div>
 </CardHeader>
 <CardContent className="space-y-3">
 <div className="grid grid-cols-2 gap-3">
 <div>
 <label className="block text-xs font-medium mb-1">Date</label>
 <input type="date" value={ehDate} onChange={(e) => setEhDate(e.target.value)}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm" />
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">Hours</label>
 <input type="number" value={ehHours} onChange={(e) => setEhHours(parseFloat(e.target.value) || 0)}
 min={0} max={48} step={0.5}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm" />
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">Start Time</label>
 <input type="time" value={ehStart} onChange={(e) => setEhStart(e.target.value)}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm" />
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">Pay Code</label>
 <select value={ehPayCode} onChange={(e) => setEhPayCode(e.target.value)}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm">
 <option value="">— Select —</option>
 {payCodes.filter((p) => p.category === 'work').map((pc) => (
 <option key={pc.id} value={pc.id}>{pc.display_name} ({pc.code})</option>
 ))}
 {payCodes.filter((p) => p.category === 'leave').map((pc) => (
 <option key={pc.id} value={pc.id}>{pc.display_name} ({pc.code})</option>
 ))}
 </select>
 </div>
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">Notes</label>
 <textarea value={ehNotes} onChange={(e) => setEhNotes(e.target.value)} rows={2}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm" placeholder="Optional..." />
 </div>
 <div className="flex justify-end gap-2">
 <Button variant="outline" size="sm" onClick={() => setShowEnterHour(false)}>Cancel</Button>
 <Button size="sm" onClick={handleEnterHour} disabled={ehSaving || !ehPayCode || ehHours <= 0}>
 {ehSaving ? <Loader2 className="w-4 h-4 animate-spin mr-1" /> : <Plus className="w-4 h-4 mr-1" />}
 Create
 </Button>
 </div>
 </CardContent>
 </Card>
 </div>
 )}
 </div>
 )
}
