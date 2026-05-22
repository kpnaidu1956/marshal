import { useState, useEffect, useCallback } from 'react'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import {
 Loader2,
 RefreshCw,
 Download,
 Clock,
 AlertTriangle,
 ShieldCheck,
 Palmtree,
 DollarSign,
} from 'lucide-react'

type TabKey = 'hours' | 'overtime' | 'flsa' | 'leave' | 'payroll'

interface HoursRow {
 employee_id: string
 employee_name: string
 rank: string
 total_hours: number
 regular_hours: number
 overtime_hours: number
 leave_hours: number
}

interface FlsaRow {
 employee_id: string
 employee_name: string
 flsa_hours: number
 threshold: number
 is_compliant: boolean
}

interface LeaveRow {
 employee_id: string
 employee_name: string
 leave_type: string
 balance_hours: number
 accrual_rate: number
 max_balance: number | null
}

interface PayrollRow {
 [key: string]: unknown
}

interface PeriodOption {
 id: string
 label: string
 period_start: string
 period_end: string
 status: string
}

function todayStr(): string {
 return new Date().toISOString().slice(0, 10)
}

function thirtyDaysAgo(): string {
 const d = new Date()
 d.setDate(d.getDate() - 30)
 return d.toISOString().slice(0, 10)
}

function downloadCsv(rows: Record<string, unknown>[], filename: string) {
 if (rows.length === 0) return
 const headers = Object.keys(rows[0])
 const csvLines = [
 headers.join(','),
 ...rows.map((row) =>
 headers
 .map((h) => {
 const v = row[h]
 const s = v == null ? '' : String(v)
 return s.includes(',') || s.includes('"') ? `"${s.replace(/"/g, '""')}"` : s
 })
 .join(','),
 ),
 ]
 const blob = new Blob([csvLines.join('\n')], { type: 'text/csv' })
 const url = URL.createObjectURL(blob)
 const a = document.createElement('a')
 a.href = url
 a.download = filename
 a.click()
 URL.revokeObjectURL(url)
 toast.success('CSV downloaded')
}

export function TimekeepingReportsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [tab, setTab] = useState<TabKey>('hours')
 const [loading, setLoading] = useState(false)
 const [error, setError] = useState<string | null>(null)

 // Date range
 const [startDate, setStartDate] = useState(thirtyDaysAgo)
 const [endDate, setEndDate] = useState(todayStr)

 // FLSA
 const [cycleStart, setCycleStart] = useState(thirtyDaysAgo)

 // Hours + Overtime data
 const [hoursData, setHoursData] = useState<HoursRow[]>([])
 const [overtimeData, setOvertimeData] = useState<HoursRow[]>([])

 // FLSA data
 const [flsaData, setFlsaData] = useState<FlsaRow[]>([])

 // Leave balances
 const [leaveData, setLeaveData] = useState<LeaveRow[]>([])

 // Payroll
 const [periods, setPeriods] = useState<PeriodOption[]>([])
 const [selectedPeriod, setSelectedPeriod] = useState('')
 const [payrollData, setPayrollData] = useState<PayrollRow[]>([])

 const fetchPeriods = useCallback(async () => {
 if (!token || !orgSlug) return
 try {
 const client = new BpeClient(token)
 const res = await client.tkListPeriods(orgSlug)
 const mapped = (res.data as PeriodOption[]).map((p) => ({
 ...p,
 label: `${p.period_start} - ${p.period_end} (${p.status})`,
 }))
 setPeriods(mapped)
 } catch {
 // ignore
 }
 }, [token, orgSlug])

 useEffect(() => {
 fetchPeriods()
 }, [fetchPeriods])

 const fetchHours = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkHoursReport(orgSlug, { start: startDate, end: endDate })
 setHoursData(res.data as HoursRow[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load hours report')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug, startDate, endDate])

 const fetchOvertime = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkOvertimeReport(orgSlug, { start: startDate, end: endDate })
 setOvertimeData(res.data as HoursRow[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load overtime report')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug, startDate, endDate])

 const fetchFlsa = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkFlsaReport(orgSlug, cycleStart)
 const d = res.data as { employees?: FlsaRow[] }
 setFlsaData(d.employees ?? (Array.isArray(res.data) ? (res.data as FlsaRow[]) : []))
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load FLSA report')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug, cycleStart])

 const fetchLeave = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkListLeaveBalances(orgSlug)
 setLeaveData(res.data as LeaveRow[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load leave balances')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 const fetchPayroll = useCallback(async () => {
 if (!token || !orgSlug || !selectedPeriod) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkPayrollExport(orgSlug, selectedPeriod)
 setPayrollData(res.data as PayrollRow[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load payroll data')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug, selectedPeriod])

 const handleRefresh = () => {
 switch (tab) {
 case 'hours':
 fetchHours()
 break
 case 'overtime':
 fetchOvertime()
 break
 case 'flsa':
 fetchFlsa()
 break
 case 'leave':
 fetchLeave()
 break
 case 'payroll':
 fetchPayroll()
 break
 }
 }

 useEffect(() => {
 handleRefresh()
 // eslint-disable-next-line react-hooks/exhaustive-deps
 }, [tab])

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view timekeeping reports.</p>
 </div>
 )
 }

 const TABS: { key: TabKey; label: string; icon: React.ReactNode }[] = [
 { key: 'hours', label: 'Hours', icon: <Clock className="w-4 h-4" /> },
 { key: 'overtime', label: 'Overtime', icon: <AlertTriangle className="w-4 h-4" /> },
 { key: 'flsa', label: 'FLSA Compliance', icon: <ShieldCheck className="w-4 h-4" /> },
 { key: 'leave', label: 'Leave Balances', icon: <Palmtree className="w-4 h-4" /> },
 { key: 'payroll', label: 'Payroll Export', icon: <DollarSign className="w-4 h-4" /> },
 ]

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Timekeeping Reports</h1>
 <Button variant="outline" size="sm" onClick={handleRefresh} disabled={loading}>
 {loading ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <RefreshCw className="w-4 h-4 mr-2" />}
 Refresh
 </Button>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Tabs */}
 <div className="flex gap-2 border-b border-gray-200 pb-1 overflow-x-auto">
 {TABS.map((t) => (
 <button
 key={t.key}
 onClick={() => setTab(t.key)}
 className={`flex items-center gap-1.5 px-4 py-2 text-sm font-medium rounded-t-lg transition-colors whitespace-nowrap ${
 tab === t.key
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 {t.icon}
 {t.label}
 </button>
 ))}
 </div>

 {/* Hours Report */}
 {tab === 'hours' && (
 <div className="space-y-4">
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-end gap-4">
 <div className="space-y-1">
 <Label htmlFor="hours-start">Start Date</Label>
 <Input id="hours-start" type="date" value={startDate} onChange={(e) => setStartDate(e.target.value)} />
 </div>
 <div className="space-y-1">
 <Label htmlFor="hours-end">End Date</Label>
 <Input id="hours-end" type="date" value={endDate} onChange={(e) => setEndDate(e.target.value)} />
 </div>
 <Button size="sm" onClick={fetchHours} disabled={loading}>
 {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Run'}
 </Button>
 {hoursData.length > 0 && (
 <Button size="sm" variant="outline" onClick={() => downloadCsv(hoursData as unknown as Record<string, unknown>[], 'hours_report.csv')}>
 <Download className="w-3.5 h-3.5 mr-1" />CSV
 </Button>
 )}
 </div>
 </CardContent>
 </Card>

 {loading ? (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 ) : hoursData.length === 0 ? (
 <div className="text-center py-12">
 <Clock className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No hours data for selected period</p>
 </div>
 ) : (
 <Card>
 <CardContent className="pt-4">
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-700">Employee</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Rank</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Total Hours</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Regular</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">OT</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Leave</th>
 </tr>
 </thead>
 <tbody>
 {hoursData.map((row) => (
 <tr key={row.employee_id} className="border-b border-gray-100">
 <td className="py-2 px-3 text-gray-900 font-medium">{row.employee_name}</td>
 <td className="py-2 px-3 text-gray-600">{row.rank}</td>
 <td className="py-2 px-3 text-right text-gray-900 font-semibold">{row.total_hours?.toFixed(1)}</td>
 <td className="py-2 px-3 text-right text-gray-600">{row.regular_hours?.toFixed(1)}</td>
 <td className="py-2 px-3 text-right">
 {row.overtime_hours > 0 ? (
 <span className="text-amber-600 font-medium">{row.overtime_hours.toFixed(1)}</span>
 ) : (
 <span className="text-gray-400">0.0</span>
 )}
 </td>
 <td className="py-2 px-3 text-right text-gray-600">{row.leave_hours?.toFixed(1)}</td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )}

 {/* Overtime Report */}
 {tab === 'overtime' && (
 <div className="space-y-4">
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-end gap-4">
 <div className="space-y-1">
 <Label htmlFor="ot-start">Start Date</Label>
 <Input id="ot-start" type="date" value={startDate} onChange={(e) => setStartDate(e.target.value)} />
 </div>
 <div className="space-y-1">
 <Label htmlFor="ot-end">End Date</Label>
 <Input id="ot-end" type="date" value={endDate} onChange={(e) => setEndDate(e.target.value)} />
 </div>
 <Button size="sm" onClick={fetchOvertime} disabled={loading}>
 {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Run'}
 </Button>
 {overtimeData.length > 0 && (
 <Button size="sm" variant="outline" onClick={() => downloadCsv(overtimeData as unknown as Record<string, unknown>[], 'overtime_report.csv')}>
 <Download className="w-3.5 h-3.5 mr-1" />CSV
 </Button>
 )}
 </div>
 </CardContent>
 </Card>

 {loading ? (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 ) : overtimeData.length === 0 ? (
 <div className="text-center py-12">
 <ShieldCheck className="w-12 h-12 mx-auto text-emerald-400 mb-3" />
 <p className="text-gray-500">No overtime recorded for this period</p>
 </div>
 ) : (
 <Card>
 <CardContent className="pt-4">
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-700">Employee</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Rank</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Total Hours</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">OT Hours</th>
 </tr>
 </thead>
 <tbody>
 {overtimeData.map((row) => (
 <tr key={row.employee_id} className="border-b border-gray-100">
 <td className="py-2 px-3 text-gray-900 font-medium">{row.employee_name}</td>
 <td className="py-2 px-3 text-gray-600">{row.rank}</td>
 <td className="py-2 px-3 text-right text-gray-900">{row.total_hours?.toFixed(1)}</td>
 <td className="py-2 px-3 text-right">
 <span className="text-amber-600 font-semibold">{row.overtime_hours?.toFixed(1)}</span>
 </td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )}

 {/* FLSA Compliance */}
 {tab === 'flsa' && (
 <div className="space-y-4">
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-end gap-4">
 <div className="space-y-1">
 <Label htmlFor="flsa-cycle">28-Day Cycle Start</Label>
 <Input id="flsa-cycle" type="date" value={cycleStart} onChange={(e) => setCycleStart(e.target.value)} />
 </div>
 <Button size="sm" onClick={fetchFlsa} disabled={loading}>
 {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Run'}
 </Button>
 </div>
 <p className="text-xs text-gray-500 mt-2">
 FLSA 7(k) exemption: 212-hour threshold over a 28-day work period for fire protection employees.
 </p>
 </CardContent>
 </Card>

 {loading ? (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 ) : flsaData.length === 0 ? (
 <div className="text-center py-12">
 <ShieldCheck className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No FLSA data for selected cycle</p>
 </div>
 ) : (
 <Card>
 <CardContent className="pt-4">
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-700">Employee</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">FLSA Hours</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Threshold</th>
 <th className="text-center py-2 px-3 font-medium text-gray-700">Status</th>
 </tr>
 </thead>
 <tbody>
 {flsaData.map((row) => (
 <tr key={row.employee_id} className="border-b border-gray-100">
 <td className="py-2 px-3 text-gray-900 font-medium">{row.employee_name}</td>
 <td className="py-2 px-3 text-right font-semibold text-gray-900">{row.flsa_hours?.toFixed(1)}</td>
 <td className="py-2 px-3 text-right text-gray-600">{row.threshold ?? 212}</td>
 <td className="py-2 px-3 text-center">
 {row.is_compliant ? (
 <Badge className="bg-emerald-100 text-emerald-700">
 Compliant
 </Badge>
 ) : (
 <Badge className="bg-red-100 text-red-700">
 Over Threshold
 </Badge>
 )}
 </td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )}

 {/* Leave Balances */}
 {tab === 'leave' && (
 <div className="space-y-4">
 <div className="flex justify-end">
 <Button size="sm" variant="outline" onClick={fetchLeave} disabled={loading}>
 {loading ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <RefreshCw className="w-4 h-4 mr-2" />}
 Refresh
 </Button>
 </div>

 {loading ? (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 ) : leaveData.length === 0 ? (
 <div className="text-center py-12">
 <Palmtree className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No leave balances found</p>
 </div>
 ) : (
 <Card>
 <CardContent className="pt-4">
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-700">Employee</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Leave Type</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Balance (hrs)</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Accrual Rate</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Max</th>
 </tr>
 </thead>
 <tbody>
 {leaveData.map((row, i) => (
 <tr key={`${row.employee_id}-${row.leave_type}-${i}`} className="border-b border-gray-100">
 <td className="py-2 px-3 text-gray-900 font-medium">{row.employee_name}</td>
 <td className="py-2 px-3">
 <Badge variant="outline">{row.leave_type}</Badge>
 </td>
 <td className="py-2 px-3 text-right font-semibold text-gray-900">{row.balance_hours?.toFixed(1)}</td>
 <td className="py-2 px-3 text-right text-gray-600">{row.accrual_rate}</td>
 <td className="py-2 px-3 text-right text-gray-600">{row.max_balance ?? '—'}</td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )}

 {/* Payroll Export */}
 {tab === 'payroll' && (
 <div className="space-y-4">
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-end gap-4">
 <div className="space-y-1 flex-1 max-w-xs">
 <Label htmlFor="payroll-period">Timecard Period</Label>
 <select
 id="payroll-period"
 value={selectedPeriod}
 onChange={(e) => setSelectedPeriod(e.target.value)}
 className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
 >
 <option value="">Select a period...</option>
 {periods.map((p) => (
 <option key={p.id} value={p.id}>{p.label}</option>
 ))}
 </select>
 </div>
 <Button size="sm" onClick={fetchPayroll} disabled={loading || !selectedPeriod}>
 {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Load'}
 </Button>
 {payrollData.length > 0 && (
 <Button
 size="sm"
 onClick={() => downloadCsv(payrollData, `payroll_export_${selectedPeriod}.csv`)}
 >
 <Download className="w-4 h-4 mr-1" />Export CSV
 </Button>
 )}
 </div>
 </CardContent>
 </Card>

 {loading ? (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 ) : payrollData.length === 0 ? (
 <div className="text-center py-12">
 <DollarSign className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">{selectedPeriod ? 'No payroll data for selected period' : 'Select a period to export payroll data'}</p>
 </div>
 ) : (
 <Card>
 <CardHeader>
 <CardTitle className="text-lg flex items-center gap-2">
 <DollarSign className="w-5 h-5" />
 Payroll Data &mdash; {payrollData.length} rows
 </CardTitle>
 </CardHeader>
 <CardContent>
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 {Object.keys(payrollData[0]).map((col) => (
 <th key={col} className="text-left py-2 px-3 font-medium text-gray-700">{col}</th>
 ))}
 </tr>
 </thead>
 <tbody>
 {payrollData.map((row, i) => (
 <tr key={i} className="border-b border-gray-100">
 {Object.values(row).map((val, j) => (
 <td key={j} className="py-2 px-3 text-gray-700">
 {val == null ? <span className="text-gray-400 italic">--</span> : String(val)}
 </td>
 ))}
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )}
 </div>
 )
}
