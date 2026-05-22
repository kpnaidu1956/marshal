import { useState, useEffect, useCallback, useMemo } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
 Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from '@/components/ui/dialog'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { WorkflowBuilder } from '@/components/bpe/WorkflowBuilder'
import { ExecutionProgressDialog } from '@/components/bpe/ExecutionProgressDialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import {
 Loader2, Play, Pause, StopCircle, RotateCcw, ChevronDown, ChevronUp,
 RefreshCw, Plus, Pencil, Trash2, Eye,
} from 'lucide-react'
import type { WorkflowDefinition, WorkflowExecution, StepTemplate, TimelineEvent } from '@/models/bpe'

const STATUS_COLORS: Record<string, string> = {
 pending: 'bg-gray-100 text-gray-700',
 running: 'bg-blue-100 text-blue-700',
 paused: 'bg-amber-100 text-amber-700',
 completed: 'bg-emerald-100 text-emerald-700',
 failed: 'bg-red-100 text-red-700',
 cancelled: 'bg-gray-100 text-gray-700',
}

function StatusBadge({ status }: { status: string }) {
 return (
 <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_COLORS[status] || STATUS_COLORS.pending}`}>
 {status}
 </span>
 )
}

export function BpeWorkflowsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [tab, setTab] = useState<'definitions' | 'executions'>('definitions')
 const [definitions, setDefinitions] = useState<WorkflowDefinition[]>([])
 const [executions, setExecutions] = useState<WorkflowExecution[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [expandedExec, setExpandedExec] = useState<string | null>(null)
 const [timeline, setTimeline] = useState<TimelineEvent[]>([])
 const [timelineLoading, setTimelineLoading] = useState(false)
 const [actionLoading, setActionLoading] = useState<string | null>(null)

 // Progress dialog state
 const [progressOpen, setProgressOpen] = useState(false)
 const [progressExecId, setProgressExecId] = useState<string | null>(null)

 // Builder dialog state
 const [builderOpen, setBuilderOpen] = useState(false)
 const [builderMode, setBuilderMode] = useState<'create' | 'edit'>('create')
 const [editingDef, setEditingDef] = useState<WorkflowDefinition | null>(null)
 const [editingId, setEditingId] = useState<string | null>(null)
 const [formLoading, setFormLoading] = useState(false)

 const client = useMemo(() => token ? new BpeClient(token) : null, [token])

 // Confirm dialog state
 const [confirmOpen, setConfirmOpen] = useState(false)
 const [confirmAction, setConfirmAction] = useState<{ title: string; description: string; onConfirm: () => Promise<void>; variant: 'danger' | 'warning' | 'default' }>({
 title: '', description: '', onConfirm: async () => {}, variant: 'default',
 })

 const fetchData = useCallback(async () => {
 if (!client || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const [defs, execs] = await Promise.all([
 client.listDefinitions(orgSlug),
 client.listExecutions(orgSlug),
 ])
 setDefinitions(defs.data)
 setExecutions(execs.data)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load workflows')
 } finally {
 setLoading(false)
 }
 }, [client, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 const toggleTimeline = async (execId: string) => {
 if (expandedExec === execId) {
 setExpandedExec(null)
 return
 }
 setExpandedExec(execId)
 setTimelineLoading(true)
 try {
 const res = await client!.executionTimeline(execId)
 setTimeline(res.data)
 } catch {
 setTimeline([])
 } finally {
 setTimelineLoading(false)
 }
 }

 const execAction = async (execId: string, action: 'start' | 'pause' | 'resume' | 'cancel') => {
 if (!client) return

 if (action === 'cancel') {
 setConfirmAction({
 title: 'Cancel Execution',
 description: 'Are you sure you want to cancel this workflow execution? This action cannot be undone.',
 variant: 'danger',
 onConfirm: async () => {
 setActionLoading(execId)
 try {
 await client!.cancelExecution(execId)
 toast.success('Execution cancelled')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to cancel execution')
 } finally {
 setActionLoading(null)
 }
 },
 })
 setConfirmOpen(true)
 return
 }

 setActionLoading(execId)
 try {
 if (action === 'start') await client!.startExecution(execId)
 else if (action === 'pause') await client!.pauseExecution(execId)
 else if (action === 'resume') await client!.resumeExecution(execId)
 const labels: Record<string, string> = { start: 'started', pause: 'paused', resume: 'resumed' }
 toast.success(`Execution ${labels[action]}`)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Action failed')
 } finally {
 setActionLoading(null)
 }
 }

 const executeDefinition = async (defId: string) => {
 if (!client || !orgSlug) return
 setActionLoading(defId)
 try {
 const res = await client.executeDefinition(defId, { organization_id: orgSlug })
 toast.success('Workflow execution created')
 // Open progress dialog for the new execution
 setProgressExecId(res.data.id)
 setProgressOpen(true)
 setTab('executions')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to execute workflow')
 } finally {
 setActionLoading(null)
 }
 }

 // --- Create / Edit ---
 const openCreateDialog = () => {
 setBuilderMode('create')
 setEditingDef(null)
 setEditingId(null)
 setBuilderOpen(true)
 }

 const openEditDialog = (def: WorkflowDefinition) => {
 setBuilderMode('edit')
 setEditingDef(def)
 setEditingId(def.id)
 setBuilderOpen(true)
 }

 const handleBuilderSubmit = async (data: {
 name: string
 description: string | null
 category: string
 step_templates: StepTemplate[]
 }) => {
 if (!client || !orgSlug) return
 setFormLoading(true)
 try {
 const body = {
 ...data,
 organization_id: orgSlug,
 }

 if (builderMode === 'edit' && editingId) {
 await client!.updateDefinition(editingId, body)
 toast.success('Workflow updated')
 } else {
 await client!.createDefinition(body)
 toast.success('Workflow created')
 }

 setBuilderOpen(false)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : `Failed to ${builderMode} workflow`)
 } finally {
 setFormLoading(false)
 }
 }

 // --- Delete ---
 const confirmDeleteDefinition = (def: WorkflowDefinition) => {
 setConfirmAction({
 title: 'Delete Workflow',
 description: `Are you sure you want to delete "${def.name}"? This action cannot be undone.`,
 variant: 'danger',
 onConfirm: async () => {
 if (!client) return
 try {
 await client.deleteDefinition(def.id)
 toast.success('Workflow deleted')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to delete workflow')
 }
 },
 })
 setConfirmOpen(true)
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view workflows.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Workflows</h1>
 <div className="flex items-center gap-2">
 <Button size="sm" onClick={openCreateDialog}>
 <Plus className="w-4 h-4 mr-2" />New Workflow
 </Button>
 <Button variant="outline" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4 mr-2" />Refresh
 </Button>
 </div>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Tabs */}
 <div className="flex gap-2 border-b border-gray-200 pb-1">
 <button
 onClick={() => setTab('definitions')}
 className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors ${
 tab === 'definitions'
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 Definitions ({definitions.length})
 </button>
 <button
 onClick={() => setTab('executions')}
 className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors ${
 tab === 'executions'
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 Executions ({executions.length})
 </button>
 </div>

 {/* Definitions Tab */}
 {tab === 'definitions' && (
 <div className="grid gap-4">
 {definitions.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-8">No workflow definitions yet</p>
 ) : definitions.map((def) => (
 <Card key={def.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-start justify-between">
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2 mb-1">
 <h3 className="font-semibold text-gray-900 truncate">{def.name}</h3>
 <Badge variant="secondary">{def.category}</Badge>
 <Badge variant="outline">v{def.version}</Badge>
 {def.source !== 'manual' && <Badge variant="secondary">{def.source}</Badge>}
 </div>
 {def.description && (
 <p className="text-sm text-gray-500 mb-2">{def.description}</p>
 )}
 <div className="flex items-center gap-4 text-xs text-gray-400">
 <span>Used {def.times_used}x</span>
 {def.success_rate != null && (
 <span>Success: {(def.success_rate * 100).toFixed(0)}%</span>
 )}
 {def.avg_completion_minutes != null && (
 <span>Avg: {def.avg_completion_minutes.toFixed(1)}m</span>
 )}
 <span>{Array.isArray(def.step_templates) ? def.step_templates.length : 0} steps</span>
 </div>
 </div>
 <div className="flex items-center gap-1.5 ml-3">
 <Button
 size="sm"
 variant="ghost"
 onClick={() => openEditDialog(def)}
 title="Edit"
 >
 <Pencil className="w-4 h-4" />
 </Button>
 <Button
 size="sm"
 variant="ghost"
 onClick={() => confirmDeleteDefinition(def)}
 title="Delete"
 className="text-red-500 hover:text-red-700 hover:bg-red-50"
 >
 <Trash2 className="w-4 h-4" />
 </Button>
 <Button
 size="sm"
 onClick={() => executeDefinition(def.id)}
 disabled={actionLoading === def.id}
 >
 {actionLoading === def.id ? (
 <Loader2 className="w-4 h-4 animate-spin" />
 ) : (
 <><Play className="w-4 h-4 mr-1" />Execute</>
 )}
 </Button>
 </div>
 </div>
 </CardContent>
 </Card>
 ))}
 </div>
 )}

 {/* Executions Tab */}
 {tab === 'executions' && (
 <div className="space-y-3">
 {executions.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-8">No workflow executions yet</p>
 ) : executions.map((exec) => (
 <Card key={exec.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <StatusBadge status={exec.status} />
 <span className="text-sm font-mono text-gray-500">{exec.id.slice(0, 8)}</span>
 <span className="text-xs text-gray-400">
 {new Date(exec.created_at).toLocaleString()}
 </span>
 </div>
 <div className="flex items-center gap-2">
 {exec.status === 'pending' && (
 <Button size="sm" variant="outline" onClick={() => execAction(exec.id, 'start')} disabled={actionLoading === exec.id}>
 <Play className="w-3.5 h-3.5 mr-1" />Start
 </Button>
 )}
 {exec.status === 'running' && (
 <Button size="sm" variant="outline" onClick={() => execAction(exec.id, 'pause')} disabled={actionLoading === exec.id}>
 <Pause className="w-3.5 h-3.5 mr-1" />Pause
 </Button>
 )}
 {exec.status === 'paused' && (
 <Button size="sm" variant="outline" onClick={() => execAction(exec.id, 'resume')} disabled={actionLoading === exec.id}>
 <RotateCcw className="w-3.5 h-3.5 mr-1" />Resume
 </Button>
 )}
 {['pending', 'running', 'paused'].includes(exec.status) && (
 <Button size="sm" variant="outline" onClick={() => execAction(exec.id, 'cancel')} disabled={actionLoading === exec.id}>
 <StopCircle className="w-3.5 h-3.5 mr-1" />Cancel
 </Button>
 )}
 <Button
 size="sm"
 variant="outline"
 onClick={() => { setProgressExecId(exec.id); setProgressOpen(true) }}
 >
 <Eye className="w-3.5 h-3.5 mr-1" />Progress
 </Button>
 <Button size="sm" variant="ghost" onClick={() => toggleTimeline(exec.id)}>
 {expandedExec === exec.id ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
 </Button>
 </div>
 </div>

 {/* Timeline */}
 {expandedExec === exec.id && (
 <div className="mt-4 border-t border-gray-100 pt-3">
 {timelineLoading ? (
 <div className="flex justify-center py-3"><Loader2 className="w-4 h-4 animate-spin" /></div>
 ) : timeline.length === 0 ? (
 <p className="text-xs text-gray-400 text-center py-2">No timeline events</p>
 ) : (
 <div className="space-y-2">
 {timeline.map((evt, i) => (
 <div key={i} className="flex items-start gap-3 text-sm">
 <span className="text-xs text-gray-400 w-36 flex-shrink-0">
 {new Date(evt.timestamp).toLocaleString()}
 </span>
 <Badge variant="outline" className="flex-shrink-0">{evt.event_type}</Badge>
 <span className="text-gray-600">{evt.description}</span>
 </div>
 ))}
 </div>
 )}
 </div>
 )}
 </CardContent>
 </Card>
 ))}
 </div>
 )}

 {/* Workflow Builder Dialog */}
 <Dialog open={builderOpen} onOpenChange={setBuilderOpen}>
 <DialogContent className="sm:max-w-3xl max-h-[85vh] overflow-y-auto">
 <DialogHeader>
 <DialogTitle>{builderMode === 'edit' ? 'Edit Workflow' : 'New Workflow'}</DialogTitle>
 <DialogDescription>
 {builderMode === 'edit'
 ? 'Update the workflow steps and configuration below.'
 : 'Design your workflow by adding steps, assigning roles, and configuring dependencies.'}
 </DialogDescription>
 </DialogHeader>
 <WorkflowBuilder
 mode={builderMode}
 definition={editingDef}
 onSubmit={handleBuilderSubmit}
 loading={formLoading}
 />
 </DialogContent>
 </Dialog>

 {/* Execution Progress Dialog */}
 <ExecutionProgressDialog
 open={progressOpen}
 onOpenChange={setProgressOpen}
 executionId={progressExecId}
 client={client}
 onStatusChange={fetchData}
 />

 {/* Confirm Dialog for destructive actions */}
 <ConfirmDialog
 open={confirmOpen}
 onOpenChange={setConfirmOpen}
 title={confirmAction.title}
 description={confirmAction.description}
 confirmLabel="Confirm"
 variant={confirmAction.variant}
 onConfirm={confirmAction.onConfirm}
 />
 </div>
 )
}
