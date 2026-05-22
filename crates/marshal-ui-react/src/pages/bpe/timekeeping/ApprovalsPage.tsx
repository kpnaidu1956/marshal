import { useState, useEffect, useCallback } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
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
 CheckCircle,
 XCircle,
 ClipboardCheck,
 CheckSquare,
} from 'lucide-react'

interface PendingTimecard {
 employee_id: string
 employee_name: string
 period_id: string
 period_start: string
 period_end: string
 total_hours: number
 certified_at: string | null
}

export function ApprovalsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [pending, setPending] = useState<PendingTimecard[]>([])

 // Bulk selection
 const [selected, setSelected] = useState<Set<string>>(new Set())

 // Decision state
 const [deciding, setDeciding] = useState<string | null>(null)

 // Reject dialog
 const [rejectTarget, setRejectTarget] = useState<PendingTimecard | null>(null)
 const [rejectNotes, setRejectNotes] = useState('')
 const [rejecting, setRejecting] = useState(false)

 const fetchPending = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkPendingApprovals(orgSlug)
 setPending(res.data as PendingTimecard[])
 setSelected(new Set())
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load pending approvals')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => {
 fetchPending()
 }, [fetchPending])

 const cardKey = (card: PendingTimecard) => `${card.employee_id}:${card.period_id}`

 const toggleSelect = (card: PendingTimecard) => {
 const key = cardKey(card)
 setSelected((prev) => {
 const next = new Set(prev)
 if (next.has(key)) {
 next.delete(key)
 } else {
 next.add(key)
 }
 return next
 })
 }

 const toggleAll = () => {
 if (selected.size === pending.length) {
 setSelected(new Set())
 } else {
 setSelected(new Set(pending.map(cardKey)))
 }
 }

 const approve = async (card: PendingTimecard) => {
 if (!token || !orgSlug) return
 const key = cardKey(card)
 setDeciding(key)
 try {
 const client = new BpeClient(token)
 await client.tkDecideTimecard({
 organization_id: orgSlug,
 employee_id: card.employee_id,
 period_id: card.period_id,
 decision: 'approved',
 })
 toast.success(`Approved timecard for ${card.employee_name}`)
 await fetchPending()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Approval failed')
 } finally {
 setDeciding(null)
 }
 }

 const openReject = (card: PendingTimecard) => {
 setRejectTarget(card)
 setRejectNotes('')
 }

 const submitReject = async () => {
 if (!token || !orgSlug || !rejectTarget) return
 setRejecting(true)
 try {
 const client = new BpeClient(token)
 await client.tkDecideTimecard({
 organization_id: orgSlug,
 employee_id: rejectTarget.employee_id,
 period_id: rejectTarget.period_id,
 decision: 'rejected',
 notes: rejectNotes.trim() || undefined,
 })
 toast.success(`Rejected timecard for ${rejectTarget.employee_name}`)
 setRejectTarget(null)
 await fetchPending()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Rejection failed')
 } finally {
 setRejecting(false)
 }
 }

 const bulkApprove = async () => {
 if (!token || !orgSlug || selected.size === 0) return
 setDeciding('bulk')
 const toApprove = pending.filter((c) => selected.has(cardKey(c)))
 let succeeded = 0
 let failed = 0
 const client = new BpeClient(token)

 for (const card of toApprove) {
 try {
 await client.tkDecideTimecard({
 organization_id: orgSlug,
 employee_id: card.employee_id,
 period_id: card.period_id,
 decision: 'approved',
 })
 succeeded++
 } catch {
 failed++
 }
 }

 if (failed > 0) {
 toast.warning(`Approved ${succeeded}, failed ${failed}`)
 } else {
 toast.success(`Approved ${succeeded} timecard(s)`)
 }

 setDeciding(null)
 await fetchPending()
 }

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view timecard approvals.</p>
 </div>
 )
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Timecard Approvals</h1>
 <div className="flex gap-2">
 {selected.size > 0 && (
 <Button size="sm" onClick={bulkApprove} disabled={deciding === 'bulk'}>
 {deciding === 'bulk' ? (
 <Loader2 className="w-4 h-4 mr-2 animate-spin" />
 ) : (
 <CheckSquare className="w-4 h-4 mr-2" />
 )}
 Approve Selected ({selected.size})
 </Button>
 )}
 <Button variant="outline" size="sm" onClick={fetchPending}>
 <RefreshCw className="w-4 h-4 mr-2" />Refresh
 </Button>
 </div>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {pending.length === 0 ? (
 <div className="text-center py-12">
 <ClipboardCheck className="w-12 h-12 mx-auto text-emerald-400 mb-3" />
 <p className="text-gray-500">All timecards are up to date</p>
 <p className="text-xs text-gray-400 mt-1">No pending approvals</p>
 </div>
 ) : (
 <>
 {/* Select All */}
 <div className="flex items-center gap-3">
 <label className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 transition-colors cursor-pointer">
 <input
 type="checkbox"
 checked={selected.size === pending.length && pending.length > 0}
 onChange={toggleAll}
 className="rounded border-gray-300"
 />
 Select All ({pending.length})
 </label>
 </div>

 {/* Pending Queue */}
 <div className="space-y-3">
 {pending.map((card) => {
 const key = cardKey(card)
 const isDeciding = deciding === key
 return (
 <Card key={key}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center gap-4">
 <input
 type="checkbox"
 checked={selected.has(key)}
 onChange={() => toggleSelect(card)}
 className="rounded border-gray-300 flex-shrink-0"
 />
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2 mb-1">
 <h3 className="font-semibold text-gray-900">{card.employee_name}</h3>
 <Badge variant="outline" className="text-xs">
 {card.period_start} &mdash; {card.period_end}
 </Badge>
 </div>
 <div className="flex items-center gap-4 text-sm text-gray-600">
 <span>Total: <strong className="text-gray-900">{card.total_hours?.toFixed(1)} hrs</strong></span>
 {card.certified_at && (
 <span>Certified: {new Date(card.certified_at).toLocaleDateString()}</span>
 )}
 </div>
 </div>
 <div className="flex items-center gap-2 flex-shrink-0">
 <Button
 size="sm"
 onClick={() => approve(card)}
 disabled={isDeciding || deciding === 'bulk'}
 >
 {isDeciding ? (
 <Loader2 className="w-4 h-4 mr-1 animate-spin" />
 ) : (
 <CheckCircle className="w-4 h-4 mr-1" />
 )}
 Approve
 </Button>
 <Button
 size="sm"
 variant="destructive"
 onClick={() => openReject(card)}
 disabled={isDeciding || deciding === 'bulk'}
 >
 <XCircle className="w-4 h-4 mr-1" />
 Reject
 </Button>
 </div>
 </div>
 </CardContent>
 </Card>
 )
 })}
 </div>
 </>
 )}

 {/* Reject Dialog */}
 <Dialog open={!!rejectTarget} onOpenChange={(open) => { if (!open) setRejectTarget(null) }}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>Reject Timecard</DialogTitle>
 <DialogDescription>
 {rejectTarget && (
 <>
 Rejecting timecard for <span className="font-semibold">{rejectTarget.employee_name}</span>
 {' '}({rejectTarget.period_start} &mdash; {rejectTarget.period_end})
 </>
 )}
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-3 py-2">
 <div className="space-y-1.5">
 <Label htmlFor="reject-notes">Rejection Reason</Label>
 <Textarea
 id="reject-notes"
 value={rejectNotes}
 onChange={(e) => setRejectNotes(e.target.value)}
 placeholder="Explain why this timecard is being rejected..."
 rows={3}
 />
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setRejectTarget(null)} disabled={rejecting}>Cancel</Button>
 <Button variant="destructive" onClick={submitReject} disabled={rejecting}>
 {rejecting ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <XCircle className="w-4 h-4 mr-1" />}
 Reject
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
