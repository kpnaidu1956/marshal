import { useState, useEffect, useCallback, useMemo, Fragment } from 'react'
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
 FileText,
 Clock,
 User,
 Filter,
 Download,
 Search,
} from 'lucide-react'

interface AuditEntry {
 id: number
 actor_user_id: string | null
 actor_name: string | null
 employee_id: string | null
 employee_name: string | null
 action: string
 resource_type: string
 resource_id: string | null
 before_state: Record<string, unknown> | null
 after_state: Record<string, unknown> | null
 summary: string
 created_at: string
}

interface AuditBreakdown {
 action: string
 resource_type: string
 count: number
}

const ACTION_COLORS: Record<string, string> = {
 'time_entry.created': 'bg-green-100 text-green-800',
 'time_entry.updated': 'bg-blue-100 text-blue-800',
 'time_entry.deleted': 'bg-red-100 text-red-800',
 'time_entry.submitted': 'bg-yellow-100 text-yellow-800',
 'timecard.certified': 'bg-purple-100 text-purple-800',
 'timecard.approved': 'bg-emerald-100 text-emerald-800',
 'timecard.rejected': 'bg-red-100 text-red-800',
}

// Strip UUIDs from summary text (e.g. "(d7d220ba-6062-...)" → "")
const UUID_RE = /\s*\(?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\)?/gi
function cleanSummary(s: string): string {
 return s.replace(UUID_RE, '').trim()
}

function actionBadgeClass(action: string): string {
 return ACTION_COLORS[action] ?? 'bg-gray-100 text-gray-800'
}

function formatTimestamp(ts: string): string {
 const d = new Date(ts)
 return d.toLocaleString('en-US', { month: 'short', day: 'numeric', year: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function today(): string {
 return new Date().toISOString().slice(0, 10)
}

function thirtyDaysAgo(): string {
 const d = new Date()
 d.setDate(d.getDate() - 30)
 return d.toISOString().slice(0, 10)
}

export function AuditTrailPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [entries, setEntries] = useState<AuditEntry[]>([])
 const [total, setTotal] = useState(0)
 const [page, setPage] = useState(1)
 const [perPage] = useState(50)
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Summary
 const [summary, setSummary] = useState<{ total_events: number; breakdown: AuditBreakdown[] } | null>(null)

 // Filters
 const [filterStart, setFilterStart] = useState(thirtyDaysAgo())
 const [filterEnd, setFilterEnd] = useState(today())
 const [filterAction, setFilterAction] = useState('')
 const [filterResourceType, setFilterResourceType] = useState('')
 const [filterEmployeeId, setFilterEmployeeId] = useState('')
 const [showFilters, setShowFilters] = useState(false)

 // Detail view
 const [expandedId, setExpandedId] = useState<number | null>(null)

 const client = useMemo(() => (token ? new BpeClient(token) : null), [token])

 const fetchData = useCallback(async () => {
 if (!client || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const params: Record<string, string> = {
 page: String(page),
 per_page: String(perPage),
 }
 if (filterStart) params.start = filterStart
 if (filterEnd) params.end = filterEnd
 if (filterAction) params.action = filterAction
 if (filterResourceType) params.resource_type = filterResourceType
 if (filterEmployeeId) params.employee_id = filterEmployeeId

 const [auditRes, summaryRes] = await Promise.all([
 client.tkAuditTrail(orgSlug, params),
 client.tkAuditSummary(orgSlug, {
 ...(filterStart ? { start: filterStart } : {}),
 ...(filterEnd ? { end: filterEnd } : {}),
 }),
 ])

 setEntries(auditRes.data as AuditEntry[])
 setTotal(auditRes.total)
 setSummary(summaryRes as { total_events: number; breakdown: AuditBreakdown[] })
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load audit trail')
 } finally {
 setLoading(false)
 }
 }, [client, orgSlug, page, perPage, filterStart, filterEnd, filterAction, filterResourceType, filterEmployeeId])

 useEffect(() => { fetchData() }, [fetchData])

 // CSV export
 function exportCsv() {
 const headers = ['Timestamp', 'Action', 'Resource Type', 'Employee', 'Summary', 'Before', 'After']
 const rows = entries.map((e) => [
 e.created_at,
 e.action,
 e.resource_type,
 e.employee_name ?? '',
 cleanSummary(e.summary),
 e.before_state ? JSON.stringify(e.before_state) : '',
 e.after_state ? JSON.stringify(e.after_state) : '',
 ])
 const csv = [headers, ...rows].map((r) => r.map((c) => `"${String(c).replace(/"/g, '""')}"`).join(',')).join('\n')
 const blob = new Blob([csv], { type: 'text/csv' })
 const url = URL.createObjectURL(blob)
 const a = document.createElement('a')
 a.href = url
 a.download = `audit-trail-${filterStart}-${filterEnd}.csv`
 a.click()
 URL.revokeObjectURL(url)
 }

 const totalPages = Math.ceil(total / perPage)

 // Unique actions for filter dropdown
 const uniqueActions = useMemo(() => {
 const s = new Set(entries.map((e) => e.action))
 if (summary) summary.breakdown.forEach((b) => s.add(b.action))
 return Array.from(s).sort()
 }, [entries, summary])

 const uniqueResourceTypes = useMemo(() => {
 const s = new Set(entries.map((e) => e.resource_type))
 if (summary) summary.breakdown.forEach((b) => s.add(b.resource_type))
 return Array.from(s).sort()
 }, [entries, summary])

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization.</p></div>
 }

 return (
 <div className="space-y-4">
 {/* Header */}
 <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
 <div>
 <h1 className="text-2xl font-bold tracking-tight flex items-center gap-2">
 <FileText className="w-6 h-6" />
 Audit Trail
 </h1>
 <p className="text-xs text-gray-500 mt-0.5">
 Complete history of all timecard operations
 </p>
 </div>
 <div className="flex items-center gap-2">
 <Button variant="outline" size="sm" onClick={() => setShowFilters((v) => !v)}>
 <Filter className="w-4 h-4 mr-1" /> Filters
 </Button>
 <Button variant="outline" size="sm" onClick={exportCsv} disabled={entries.length === 0}>
 <Download className="w-4 h-4 mr-1" /> Export CSV
 </Button>
 <Button variant="ghost" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4" />
 </Button>
 </div>
 </div>

 {/* Summary Cards */}
 {summary && (
 <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
 <Card>
 <CardContent className="py-3 text-center">
 <p className="text-2xl font-bold text-gray-900">{summary.total_events}</p>
 <p className="text-xs text-gray-500">Total Events</p>
 </CardContent>
 </Card>
 {summary.breakdown.slice(0, 3).map((b, i) => (
 <Card key={i}>
 <CardContent className="py-3 text-center">
 <p className="text-2xl font-bold text-gray-900">{b.count}</p>
 <p className="text-xs text-gray-500">{b.action}</p>
 </CardContent>
 </Card>
 ))}
 </div>
 )}

 {/* Filters */}
 {showFilters && (
 <Card>
 <CardContent className="py-3">
 <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
 <div>
 <label className="block text-xs font-medium mb-1">Start Date</label>
 <input type="date" value={filterStart} onChange={(e) => { setFilterStart(e.target.value); setPage(1) }}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm" />
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">End Date</label>
 <input type="date" value={filterEnd} onChange={(e) => { setFilterEnd(e.target.value); setPage(1) }}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm" />
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">Action</label>
 <select value={filterAction} onChange={(e) => { setFilterAction(e.target.value); setPage(1) }}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm">
 <option value="">All Actions</option>
 {uniqueActions.map((a) => <option key={a} value={a}>{a}</option>)}
 </select>
 </div>
 <div>
 <label className="block text-xs font-medium mb-1">Resource Type</label>
 <select value={filterResourceType} onChange={(e) => { setFilterResourceType(e.target.value); setPage(1) }}
 className="w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm">
 <option value="">All Types</option>
 {uniqueResourceTypes.map((r) => <option key={r} value={r}>{r}</option>)}
 </select>
 </div>
 <div className="flex items-end">
 <Button variant="outline" size="sm" onClick={() => { setFilterAction(''); setFilterResourceType(''); setFilterEmployeeId(''); setFilterStart(thirtyDaysAgo()); setFilterEnd(today()); setPage(1) }}>
 Clear
 </Button>
 </div>
 </div>
 </CardContent>
 </Card>
 )}

 {/* Error */}
 {error && (
 <div className="rounded-lg border border-red-300 bg-red-50 p-3 text-red-700 text-sm">
 {error}
 </div>
 )}

 {/* Audit Trail Table */}
 <Card>
 <CardHeader className="pb-2">
 <div className="flex items-center justify-between">
 <CardTitle className="text-sm">{total} events</CardTitle>
 <div className="flex items-center gap-2 text-xs text-gray-500">
 <span>Page {page} of {totalPages || 1}</span>
 <Button variant="ghost" size="sm" disabled={page <= 1} onClick={() => setPage(page - 1)}>
 <ChevronLeft className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" disabled={page >= totalPages} onClick={() => setPage(page + 1)}>
 <ChevronRight className="w-3 h-3" />
 </Button>
 </div>
 </div>
 </CardHeader>
 <CardContent className="p-0">
 {loading ? (
 <div className="flex items-center justify-center h-32">
 <Loader2 className="w-5 h-5 animate-spin text-indigo-500" />
 </div>
 ) : entries.length === 0 ? (
 <div className="text-center py-8 text-sm text-gray-500">No audit events found for this period.</div>
 ) : (
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200 bg-gray-50">
 <th className="text-left px-3 py-2 font-medium text-gray-500">Timestamp</th>
 <th className="text-left px-3 py-2 font-medium text-gray-500">Action</th>
 <th className="text-left px-3 py-2 font-medium text-gray-500">Employee</th>
 <th className="text-left px-3 py-2 font-medium text-gray-500">Summary</th>
 <th className="text-center px-3 py-2 font-medium text-gray-500">Details</th>
 </tr>
 </thead>
 <tbody>
 {entries.map((entry) => {
 const isExpanded = expandedId === entry.id
 return (
 <Fragment key={entry.id}>
 <tr
 className="border-b border-gray-100 hover:bg-gray-50 cursor-pointer"
 onClick={() => setExpandedId(isExpanded ? null : entry.id)}
 >
 <td className="px-3 py-2 text-xs text-gray-600 whitespace-nowrap">
 <Clock className="w-3 h-3 inline mr-1 -mt-0.5" />
 {formatTimestamp(entry.created_at)}
 </td>
 <td className="px-3 py-2">
 <Badge className={`text-[10px] ${actionBadgeClass(entry.action)}`}>
 {entry.action}
 </Badge>
 </td>
 <td className="px-3 py-2 text-gray-700">
 {entry.employee_name && (
 <span className="flex items-center gap-1">
 <User className="w-3 h-3" />
 {entry.employee_name}
 </span>
 )}
 </td>
 <td className="px-3 py-2 text-gray-700 max-w-sm truncate">
 {cleanSummary(entry.summary)}
 </td>
 <td className="px-3 py-2 text-center">
 {(entry.before_state || entry.after_state) && (
 <Badge variant="outline" className="text-[10px]">
 <Search className="w-3 h-3" />
 </Badge>
 )}
 </td>
 </tr>
 {isExpanded && (
 <tr key={`${entry.id}-detail`} className="bg-gray-50">
 <td colSpan={5} className="px-4 py-3">
 <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
 {entry.before_state && (
 <div>
 <p className="font-semibold text-gray-600 mb-1">Before</p>
 <pre className="bg-white border border-gray-200 rounded p-2 overflow-x-auto max-h-48 text-[11px]">
 {JSON.stringify(entry.before_state, null, 2)}
 </pre>
 </div>
 )}
 {entry.after_state && (
 <div>
 <p className="font-semibold text-gray-600 mb-1">After</p>
 <pre className="bg-white border border-gray-200 rounded p-2 overflow-x-auto max-h-48 text-[11px]">
 {JSON.stringify(entry.after_state, null, 2)}
 </pre>
 </div>
 )}
 {!entry.before_state && !entry.after_state && (
 <p className="text-gray-400">No state details recorded for this event.</p>
 )}
 <div className="md:col-span-2 text-gray-500 space-y-0.5">
 <p>Resource: <span className="font-medium">{entry.resource_type}</span></p>
 {entry.actor_name && <p>Actor: <span className="font-medium">{entry.actor_name}</span></p>}
 </div>
 </div>
 </td>
 </tr>
 )}
 </Fragment>
 )
 })}
 </tbody>
 </table>
 </div>
 )}
 </CardContent>
 </Card>
 </div>
 )
}
