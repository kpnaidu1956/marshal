import { useState, useEffect, useCallback } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import {
 Dialog,
 DialogContent,
 DialogHeader,
 DialogTitle,
 DialogFooter,
 DialogDescription,
} from '@/components/ui/dialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import {
 Loader2,
 RefreshCw,
 ShieldAlert,
 Play,
 CheckCircle,
 Filter,
} from 'lucide-react'

type FlagType = 'roster_conflict' | 'hours_exceeded' | 'excess_daily_hours' | 'missing_entry' | 'not_on_roster' | 'duplicate_entry' | 'weekend_hours' | 'staffing_below_min'
type Severity = 'info' | 'warning' | 'error'

interface ValidationFlag {
 id: string
 employee_id: string
 employee_name: string
 flag_type: FlagType
 severity: Severity
 message: string
 flag_date: string | null
 is_resolved: boolean
 resolution_note?: string | null
 created_at: string
}

const SEVERITY_STYLES: Record<Severity, string> = {
 info: 'bg-blue-100 text-blue-700',
 warning: 'bg-amber-100 text-amber-700',
 error: 'bg-red-100 text-red-700',
}

const FLAG_TYPE_LABELS: Record<string, string> = {
 roster_conflict: 'Roster Conflict',
 hours_exceeded: 'Hours Exceeded',
 excess_daily_hours: 'Excess Daily Hours',
 missing_entry: 'Missing Entry',
 not_on_roster: 'Not On Roster',
 duplicate_entry: 'Duplicate Entry',
 weekend_hours: 'Weekend Hours',
 staffing_below_min: 'Staffing Below Min',
}

function todayStr(): string {
 return new Date().toISOString().slice(0, 10)
}

function thirtyDaysAgo(): string {
 const d = new Date()
 d.setDate(d.getDate() - 30)
 return d.toISOString().slice(0, 10)
}

export function ValidationFlagsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [loading, setLoading] = useState(false)
 const [error, setError] = useState<string | null>(null)
 const [flags, setFlags] = useState<ValidationFlag[]>([])

 // Validation run
 const [validating, setValidating] = useState(false)
 const [validationSummary, setValidationSummary] = useState<{ total_flags: number; by_type: Record<string, number> } | null>(null)
 const [valStart, setValStart] = useState(thirtyDaysAgo)
 const [valEnd, setValEnd] = useState(todayStr)

 // Filters
 const [filterType, setFilterType] = useState<string>('')
 const [filterResolved, setFilterResolved] = useState<string>('')

 // Resolve dialog
 const [resolveTarget, setResolveTarget] = useState<ValidationFlag | null>(null)
 const [resolveNote, setResolveNote] = useState('')
 const [resolving, setResolving] = useState(false)

 const fetchFlags = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const params: Record<string, string> = {}
 if (filterType) params.flag_type = filterType
 if (filterResolved === 'resolved') params.resolved = 'true'
 if (filterResolved === 'unresolved') params.resolved = 'false'
 if (valStart) params.start = valStart
 if (valEnd) params.end = valEnd
 const res = await client.tkListFlags(orgSlug, params)
 setFlags(res.data as ValidationFlag[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load flags')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug, filterType, filterResolved, valStart, valEnd])

 useEffect(() => {
 fetchFlags()
 }, [fetchFlags])

 const runValidation = async () => {
 if (!token || !orgSlug) return
 setValidating(true)
 setValidationSummary(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkValidate({
 organization_id: orgSlug,
 start: valStart,
 end: valEnd,
 })
 setValidationSummary(res)
 toast.success(`Validation complete: ${res.total_flags} flag(s) found`)
 await fetchFlags()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Validation failed')
 } finally {
 setValidating(false)
 }
 }

 const handleResolve = async () => {
 if (!token || !resolveTarget) return
 setResolving(true)
 try {
 const client = new BpeClient(token)
 await client.tkResolveFlag(resolveTarget.id, { resolution_note: resolveNote.trim() || null })
 toast.success('Flag resolved')
 setResolveTarget(null)
 setResolveNote('')
 await fetchFlags()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to resolve flag')
 } finally {
 setResolving(false)
 }
 }

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view validation flags.</p>
 </div>
 )
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Validation Flags</h1>
 <Button variant="outline" size="sm" onClick={fetchFlags} disabled={loading}>
 {loading ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <RefreshCw className="w-4 h-4 mr-2" />}
 Refresh
 </Button>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Run Validation */}
 <Card>
 <CardContent className="pt-4 pb-4">
 <h2 className="text-sm font-semibold text-gray-700 mb-3">Run Validation</h2>
 <div className="flex items-end gap-4 flex-wrap">
 <div className="space-y-1">
 <Label htmlFor="val-start">Start Date</Label>
 <Input id="val-start" type="date" value={valStart} onChange={(e) => setValStart(e.target.value)} />
 </div>
 <div className="space-y-1">
 <Label htmlFor="val-end">End Date</Label>
 <Input id="val-end" type="date" value={valEnd} onChange={(e) => setValEnd(e.target.value)} />
 </div>
 <Button onClick={runValidation} disabled={validating}>
 {validating ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Play className="w-4 h-4 mr-2" />}
 Run Validation
 </Button>
 </div>

 {validationSummary && (
 <div className="mt-4 p-3 bg-gray-50 rounded-lg">
 <p className="text-sm font-medium text-gray-900 mb-2">
 Results: {validationSummary.total_flags} flag(s)
 </p>
 <div className="flex gap-3 flex-wrap">
 {Object.entries(validationSummary.by_type).map(([type, count]) => (
 <button
 key={type}
 onClick={() => { setFilterType(type); setFilterResolved('unresolved') }}
 className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs font-medium cursor-pointer transition-colors hover:bg-gray-100 ${
 filterType === type ? 'border-primary bg-primary/10 text-primary' : 'border-gray-300 text-gray-700'
 }`}
 >
 <span className={`inline-block w-2 h-2 rounded-full ${
 type.includes('exceeded') || type === 'staffing_below_min' ? 'bg-red-500' :
 type === 'missing_entry' || type === 'not_on_roster' ? 'bg-amber-500' :
 'bg-blue-500'
 }`} />
 {FLAG_TYPE_LABELS[type] || type}: <strong>{count}</strong>
 </button>
 ))}
 {filterType && (
 <button
 onClick={() => setFilterType('')}
 className="text-xs text-gray-500 hover:text-gray-700 underline"
 >
 Clear filter
 </button>
 )}
 </div>
 </div>
 )}
 </CardContent>
 </Card>

 {/* Filters */}
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-end gap-4 flex-wrap">
 <div className="flex items-center gap-2">
 <Filter className="w-4 h-4 text-gray-500" />
 <span className="text-sm font-medium text-gray-700">Filters</span>
 </div>
 <div className="space-y-1">
 <Label htmlFor="filter-type">Flag Type</Label>
 <select
 id="filter-type"
 value={filterType}
 onChange={(e) => setFilterType(e.target.value)}
 className="rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
 >
 <option value="">All Types</option>
 <option value="roster_conflict">Roster Conflict</option>
 <option value="hours_exceeded">Hours Exceeded</option>
 <option value="excess_daily_hours">Excess Daily Hours</option>
 <option value="missing_entry">Missing Entry</option>
 <option value="not_on_roster">Not On Roster</option>
 <option value="duplicate_entry">Duplicate Entry</option>
 <option value="weekend_hours">Weekend Hours</option>
 <option value="staffing_below_min">Staffing Below Min</option>
 </select>
 </div>
 <div className="space-y-1">
 <Label htmlFor="filter-resolved">Status</Label>
 <select
 id="filter-resolved"
 value={filterResolved}
 onChange={(e) => setFilterResolved(e.target.value)}
 className="rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
 >
 <option value="">All</option>
 <option value="unresolved">Unresolved</option>
 <option value="resolved">Resolved</option>
 </select>
 </div>
 </div>
 </CardContent>
 </Card>

 {/* Flags Table */}
 {loading ? (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 ) : flags.length === 0 ? (
 <div className="text-center py-12">
 <ShieldAlert className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No validation flags</p>
 <p className="text-xs text-gray-400 mt-1">Run a validation to check roster vs. time entries</p>
 </div>
 ) : (
 <Card>
 <CardContent className="pt-4">
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-700">Employee</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Flag Type</th>
 <th className="text-center py-2 px-3 font-medium text-gray-700">Severity</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Message</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Date</th>
 <th className="text-center py-2 px-3 font-medium text-gray-700">Status</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Action</th>
 </tr>
 </thead>
 <tbody>
 {flags.map((flag) => (
 <tr key={flag.id} className="border-b border-gray-100">
 <td className="py-2 px-3 text-gray-900 font-medium">{flag.employee_name}</td>
 <td className="py-2 px-3">
 <Badge variant="outline" className="text-xs">
 {FLAG_TYPE_LABELS[flag.flag_type] || flag.flag_type}
 </Badge>
 </td>
 <td className="py-2 px-3 text-center">
 <Badge className={`text-xs ${SEVERITY_STYLES[flag.severity] || SEVERITY_STYLES.info}`}>
 {flag.severity}
 </Badge>
 </td>
 <td className="py-2 px-3 text-gray-700 max-w-xs truncate">{flag.message}</td>
 <td className="py-2 px-3 text-gray-600 whitespace-nowrap">{flag.flag_date ?? '—'}</td>
 <td className="py-2 px-3 text-center">
 {flag.is_resolved ? (
 <Badge className="bg-emerald-100 text-emerald-700 text-xs">
 Resolved
 </Badge>
 ) : (
 <Badge className="bg-amber-100 text-amber-700 text-xs">
 Open
 </Badge>
 )}
 </td>
 <td className="py-2 px-3 text-right">
 {!flag.is_resolved && (
 <Button
 size="sm"
 variant="outline"
 onClick={() => {
 setResolveTarget(flag)
 setResolveNote('')
 }}
 >
 <CheckCircle className="w-3.5 h-3.5 mr-1" />Resolve
 </Button>
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

 {/* Resolve Dialog */}
 <Dialog open={!!resolveTarget} onOpenChange={(open) => { if (!open) setResolveTarget(null) }}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>Resolve Flag</DialogTitle>
 <DialogDescription>
 {resolveTarget && (
 <>
 <span className="font-medium">{resolveTarget.employee_name}</span>
 {' '}&mdash; {FLAG_TYPE_LABELS[resolveTarget.flag_type] || resolveTarget.flag_type}
 <br />
 <span className="text-xs">{resolveTarget.message}</span>
 </>
 )}
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-3 py-2">
 <div className="space-y-1.5">
 <Label htmlFor="resolve-note">Resolution Note (optional)</Label>
 <Textarea
 id="resolve-note"
 value={resolveNote}
 onChange={(e) => setResolveNote(e.target.value)}
 placeholder="Describe how this was resolved..."
 rows={3}
 />
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setResolveTarget(null)} disabled={resolving}>Cancel</Button>
 <Button onClick={handleResolve} disabled={resolving}>
 {resolving ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <CheckCircle className="w-4 h-4 mr-1" />}
 Resolve
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
