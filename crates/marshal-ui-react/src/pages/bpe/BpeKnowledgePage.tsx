import { useState, useEffect, useCallback } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import { Loader2, Brain, ArrowUpCircle, Trash2, RefreshCw, ChevronDown, ChevronUp } from 'lucide-react'
import type { LearnedSequence } from '@/models/bpe'

export function BpeKnowledgePage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [sequences, setSequences] = useState<LearnedSequence[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [promoting, setPromoting] = useState<string | null>(null)
 const [expandedId, setExpandedId] = useState<string | null>(null)
 const [deactivateId, setDeactivateId] = useState<string | null>(null)

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.listSequences(orgSlug)
 setSequences(res.data)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load sequences')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 const promote = async (id: string) => {
 if (!token || !orgSlug) return
 setPromoting(id)
 try {
 const client = new BpeClient(token)
 await client.promoteSequence(id, { organization_id: orgSlug })
 toast.success('Sequence promoted to workflow definition')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Promote failed')
 } finally {
 setPromoting(null)
 }
 }

 const handleDeactivate = async () => {
 if (!token || !deactivateId) return
 try {
 const client = new BpeClient(token)
 await client.deactivateSequence(deactivateId)
 toast.success('Sequence deactivated')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Deactivate failed')
 }
 setDeactivateId(null)
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view knowledge base.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold text-gray-900">Learned Sequences</h1>
 <p className="text-sm text-gray-500 mt-1">Patterns learned from workflow executions</p>
 </div>
 <Button variant="outline" size="sm" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Refresh</Button>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {sequences.length === 0 ? (
 <div className="text-center py-16">
 <Brain className="w-16 h-16 mx-auto text-gray-400 mb-4" />
 <p className="text-gray-500">No learned sequences yet</p>
 <p className="text-xs text-gray-500 mt-1">Complete workflow executions to generate learned patterns</p>
 </div>
 ) : (
 <div className="grid gap-4">
 {sequences.map((seq) => {
 const steps = seq.steps as unknown[]
 return (
 <Card key={seq.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-start justify-between">
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2 mb-1">
 <Brain className="w-4 h-4 text-purple-500 flex-shrink-0" />
 <h3 className="font-semibold text-gray-900 truncate">{seq.name}</h3>
 <Badge variant={seq.is_active ? 'default' : 'secondary'}>
 {seq.is_active ? 'Active' : 'Inactive'}
 </Badge>
 </div>
 {seq.description && (
 <p className="text-sm text-gray-500 mb-2">{seq.description}</p>
 )}
 <div className="flex items-center gap-4 text-xs text-gray-500">
 <button
 onClick={() => setExpandedId(expandedId === seq.id ? null : seq.id)}
 className="flex items-center gap-1 hover:text-gray-600"
 >
 {steps.length} steps
 {expandedId === seq.id ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
 </button>
 <span>Suggested {seq.times_suggested}x</span>
 <span>Accepted {seq.times_accepted}x</span>
 {seq.acceptance_rate != null && (
 <Badge variant="outline">{(seq.acceptance_rate * 100).toFixed(0)}% acceptance</Badge>
 )}
 </div>
 </div>
 <div className="flex gap-2 flex-shrink-0 ml-4">
 <Button
 size="sm"
 variant="outline"
 onClick={() => promote(seq.id)}
 disabled={promoting === seq.id}
 >
 {promoting === seq.id ? (
 <Loader2 className="w-3.5 h-3.5 animate-spin" />
 ) : (
 <><ArrowUpCircle className="w-3.5 h-3.5 mr-1" />Promote</>
 )}
 </Button>
 <Button size="sm" variant="ghost" onClick={() => setDeactivateId(seq.id)}>
 <Trash2 className="w-3.5 h-3.5 text-red-500" />
 </Button>
 </div>
 </div>

 {/* Step preview */}
 {expandedId === seq.id && steps.length > 0 && (
 <div className="mt-3 pt-3 border-t border-gray-100">
 <div className="space-y-1.5">
 {steps.map((step, i) => {
 const s = step as Record<string, unknown>
 return (
 <div key={i} className="flex items-center gap-2 text-xs text-gray-600">
 <span className="w-5 h-5 rounded-full bg-gray-100 flex items-center justify-center text-[10px] font-medium">{i + 1}</span>
 <span className="font-medium">{String(s.name || s.step_name || `Step ${i + 1}`)}</span>
 {s.step_type && <Badge variant="outline" className="text-[10px] px-1">{String(s.step_type)}</Badge>}
 </div>
 )
 })}
 </div>
 </div>
 )}
 </CardContent>
 </Card>
 )
 })}
 </div>
 )}

 <ConfirmDialog
 open={!!deactivateId}
 onOpenChange={(open) => !open && setDeactivateId(null)}
 title="Deactivate Learned Sequence"
 description="This sequence will no longer be suggested for new workflows. You can reactivate it later."
 confirmLabel="Deactivate"
 variant="warning"
 onConfirm={handleDeactivate}
 />
 </div>
 )
}
