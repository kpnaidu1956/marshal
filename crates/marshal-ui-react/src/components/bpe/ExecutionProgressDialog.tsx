import { useState, useEffect, useRef, useCallback } from 'react'
import {
 Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from '@/components/ui/dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
 Loader2, CheckCircle2, XCircle, Clock, AlertTriangle,
 SkipForward, Pause, Play, ChevronDown, ChevronUp,
} from 'lucide-react'
import type { WorkflowExecution, WorkflowStep, TimelineEvent } from '@/models/bpe'
import { BpeClient } from '@/api/bpe'

const POLL_INTERVAL = 3000

const STEP_STATUS_CONFIG: Record<string, { icon: React.ReactNode; label: string; color: string }> = {
 pending: {
 icon: <Clock className="w-4 h-4 text-gray-400" />,
 label: 'Waiting',
 color: 'text-gray-500',
 },
 ready: {
 icon: <Play className="w-4 h-4 text-blue-500" />,
 label: 'Ready to start',
 color: 'text-blue-600',
 },
 in_progress: {
 icon: <Loader2 className="w-4 h-4 text-blue-500 animate-spin" />,
 label: 'In progress',
 color: 'text-blue-600',
 },
 waiting_approval: {
 icon: <Pause className="w-4 h-4 text-amber-500" />,
 label: 'Awaiting approval',
 color: 'text-amber-600',
 },
 waiting_integration: {
 icon: <Loader2 className="w-4 h-4 text-purple-500 animate-spin" />,
 label: 'Running integration',
 color: 'text-purple-600',
 },
 completed: {
 icon: <CheckCircle2 className="w-4 h-4 text-emerald-500" />,
 label: 'Completed',
 color: 'text-emerald-600',
 },
 failed: {
 icon: <XCircle className="w-4 h-4 text-red-500" />,
 label: 'Failed',
 color: 'text-red-600',
 },
 skipped: {
 icon: <SkipForward className="w-4 h-4 text-gray-400" />,
 label: 'Skipped',
 color: 'text-gray-500',
 },
}

const EXEC_STATUS_MSG: Record<string, string> = {
 draft: 'Preparing workflow...',
 confirmed: 'Workflow confirmed, ready to start.',
 running: 'Workflow is running...',
 paused: 'Workflow is paused.',
 completed: 'Workflow completed successfully!',
 failed: 'Workflow failed. Check step details below.',
 cancelled: 'Workflow was cancelled.',
}

function friendlyStepMessage(step: WorkflowStep): string {
 const status = STEP_STATUS_CONFIG[step.status]
 const prefix = status?.label || step.status
 if (step.status === 'failed' && step.error_message) {
 return `${prefix}: ${step.error_message}`
 }
 if (step.status === 'completed' && step.completed_at) {
 const dur = step.started_at
 ? Math.round((new Date(step.completed_at).getTime() - new Date(step.started_at).getTime()) / 1000)
 : null
 return dur !== null ? `${prefix} in ${dur}s` : prefix
 }
 return prefix
}

interface ExecutionProgressDialogProps {
 open: boolean
 onOpenChange: (open: boolean) => void
 executionId: string | null
 client: BpeClient | null
 onStatusChange?: () => void
}

export function ExecutionProgressDialog({
 open, onOpenChange, executionId, client, onStatusChange,
}: ExecutionProgressDialogProps) {
 const [execution, setExecution] = useState<WorkflowExecution | null>(null)
 const [steps, setSteps] = useState<WorkflowStep[]>([])
 const [timeline, setTimeline] = useState<TimelineEvent[]>([])
 const [showTimeline, setShowTimeline] = useState(false)
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
 const prevStatusRef = useRef<string | null>(null)

 const fetchProgress = useCallback(async () => {
 if (!client || !executionId) return
 try {
 const res = await client.getExecution(executionId)
 setExecution(res.data)
 setSteps(res.steps || [])
 setError(null)

 // Notify parent when execution reaches a terminal state
 if (
 prevStatusRef.current &&
 prevStatusRef.current !== res.data.status &&
 ['completed', 'failed', 'cancelled'].includes(res.data.status)
 ) {
 onStatusChange?.()
 }
 prevStatusRef.current = res.data.status
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to fetch progress')
 } finally {
 setLoading(false)
 }
 }, [client, executionId, onStatusChange])

 const fetchTimeline = useCallback(async () => {
 if (!client || !executionId) return
 try {
 const res = await client.executionTimeline(executionId)
 setTimeline(res.data || [])
 } catch {
 setTimeline([])
 }
 }, [client, executionId])

 // Initial fetch + polling
 useEffect(() => {
 if (!open || !executionId) return

 setLoading(true)
 setExecution(null)
 setSteps([])
 setTimeline([])
 setShowTimeline(false)
 prevStatusRef.current = null
 fetchProgress()

 pollRef.current = setInterval(() => {
 fetchProgress()
 }, POLL_INTERVAL)

 return () => {
 if (pollRef.current) clearInterval(pollRef.current)
 }
 }, [open, executionId, fetchProgress])

 // Stop polling when terminal
 const execStatus = execution?.status
 useEffect(() => {
 if (execStatus && ['completed', 'failed', 'cancelled'].includes(execStatus)) {
 if (pollRef.current) {
 clearInterval(pollRef.current)
 pollRef.current = null
 }
 // Auto-fetch timeline when done
 fetchTimeline()
 }
 }, [execStatus, fetchTimeline])

 // Fetch timeline on toggle
 useEffect(() => {
 if (showTimeline && timeline.length === 0) {
 fetchTimeline()
 }
 }, [showTimeline, timeline.length, fetchTimeline])

 const completedCount = steps.filter((s) => s.status === 'completed' || s.status === 'skipped').length
 const totalSteps = steps.length
 const progressPct = totalSteps > 0 ? Math.round((completedCount / totalSteps) * 100) : 0
 const isActive = execution && ['draft', 'confirmed', 'running', 'paused'].includes(execution.status)

 return (
 <Dialog open={open} onOpenChange={onOpenChange}>
 <DialogContent className="sm:max-w-lg max-h-[85vh] overflow-y-auto">
 <DialogHeader>
 <DialogTitle className="flex items-center gap-2">
 {isActive && <Loader2 className="w-4 h-4 animate-spin text-blue-500" />}
 {execution?.status === 'completed' && <CheckCircle2 className="w-5 h-5 text-emerald-500" />}
 {execution?.status === 'failed' && <XCircle className="w-5 h-5 text-red-500" />}
 Execution Progress
 </DialogTitle>
 <DialogDescription>
 {execution ? EXEC_STATUS_MSG[execution.status] || execution.status : 'Loading...'}
 </DialogDescription>
 </DialogHeader>

 {loading && !execution ? (
 <div className="flex items-center justify-center py-8">
 <Loader2 className="w-6 h-6 animate-spin text-indigo-500" />
 </div>
 ) : error ? (
 <div className="text-sm text-red-600 bg-red-50 p-3 rounded">
 {error}
 </div>
 ) : (
 <div className="space-y-4">
 {/* Progress bar */}
 <div>
 <div className="flex justify-between text-xs text-gray-500 mb-1">
 <span>{completedCount} of {totalSteps} steps done</span>
 <span>{progressPct}%</span>
 </div>
 <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
 <div
 className={`h-full rounded-full transition-all duration-500 ${
 execution?.status === 'failed'
 ? 'bg-red-500'
 : execution?.status === 'completed'
 ? 'bg-emerald-500'
 : 'bg-blue-500'
 }`}
 style={{ width: `${progressPct}%` }}
 />
 </div>
 </div>

 {/* Steps list */}
 <div className="space-y-1.5">
 {steps.map((step) => {
 const cfg = STEP_STATUS_CONFIG[step.status] || STEP_STATUS_CONFIG.pending
 return (
 <div
 key={step.id}
 className={`flex items-start gap-2.5 p-2 rounded-lg border transition-colors ${
 step.status === 'in_progress' || step.status === 'waiting_integration'
 ? 'border-blue-200 bg-blue-50/50'
 : step.status === 'failed'
 ? 'border-red-200 bg-red-50/50'
 : step.status === 'completed'
 ? 'border-emerald-200 bg-emerald-50/30'
 : 'border-gray-200'
 }`}
 >
 <div className="mt-0.5 flex-shrink-0">{cfg.icon}</div>
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2">
 <span className="text-xs font-mono text-gray-400">#{step.step_order}</span>
 <span className="text-sm font-medium text-gray-900 truncate">
 {step.name}
 </span>
 <Badge variant="outline" className="text-[10px] flex-shrink-0">
 {step.step_type}
 </Badge>
 </div>
 <p className={`text-xs mt-0.5 ${cfg.color}`}>
 {friendlyStepMessage(step)}
 </p>
 {step.assigned_to && (
 <p className="text-[10px] text-gray-400 mt-0.5">
 Assigned to: {step.assigned_to}
 </p>
 )}
 </div>
 </div>
 )
 })}
 </div>

 {/* Execution metadata */}
 {execution && (
 <div className="flex flex-wrap gap-3 text-[10px] text-gray-400 pt-1 border-t border-gray-100">
 <span>ID: {execution.id.slice(0, 8)}</span>
 {execution.started_at && (
 <span>Started: {new Date(execution.started_at).toLocaleString()}</span>
 )}
 {execution.completed_at && (
 <span>Completed: {new Date(execution.completed_at).toLocaleString()}</span>
 )}
 </div>
 )}

 {/* Audit trail toggle */}
 <div>
 <Button
 variant="ghost"
 size="sm"
 className="w-full text-xs"
 onClick={() => setShowTimeline(!showTimeline)}
 >
 {showTimeline ? <ChevronUp className="w-3 h-3 mr-1" /> : <ChevronDown className="w-3 h-3 mr-1" />}
 Audit Trail ({timeline.length} events)
 </Button>

 {showTimeline && (
 <div className="mt-2 space-y-1.5 max-h-48 overflow-y-auto">
 {timeline.length === 0 ? (
 <p className="text-xs text-gray-400 text-center py-2">No audit events yet</p>
 ) : (
 timeline.map((evt, i) => (
 <div key={i} className="flex items-start gap-2 text-xs py-1 border-b border-gray-50 last:border-0">
 <span className="text-gray-400 w-28 flex-shrink-0 text-[10px]">
 {new Date(evt.timestamp).toLocaleTimeString()}
 </span>
 <Badge variant="outline" className="text-[10px] flex-shrink-0">{evt.event_type}</Badge>
 <span className="text-gray-600">{evt.description}</span>
 </div>
 ))
 )}
 </div>
 )}
 </div>
 </div>
 )}
 </DialogContent>
 </Dialog>
 )
}
