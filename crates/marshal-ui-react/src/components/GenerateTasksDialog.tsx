import { useState, useEffect, useCallback } from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Loader2, Sparkles, Check, X, RefreshCw, FileText, GitBranch, Brain } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { RagClient } from '@/api/rag'
import { PostgRestClient } from '@/api/postgrest'
import { detectApiUrls } from '@/lib/config'
import { generateTasksForGoal, spreadDueDates, type GeneratedTask, type GenerationResult } from '@/api/taskGenerator'
import type { Goal } from '@/models/goal'

interface Props {
 open: boolean
 onOpenChange: (open: boolean) => void
 goal: Goal
 onTasksCreated?: () => void
}

interface EditableTask extends GeneratedTask {
 selected: boolean
}

type Phase = 'idle' | 'generating' | 'review' | 'saving'

const PROGRESS_MESSAGES = [
 'Querying knowledge base for guidelines...',
 'Checking workflow patterns...',
 'Generating tasks with AI...',
]

export function GenerateTasksDialog({ open, onOpenChange, goal, onTasksCreated }: Props) {
 const token = useAuthStore((s) => s.token)
 const user = useAuthStore((s) => s.user)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [phase, setPhase] = useState<Phase>('idle')
 const [result, setResult] = useState<GenerationResult | null>(null)
 const [editTasks, setEditTasks] = useState<EditableTask[]>([])
 const [error, setError] = useState<string | null>(null)
 const [progressIdx, setProgressIdx] = useState(0)
 const [expandedDesc, setExpandedDesc] = useState<Set<number>>(new Set())
 const [citationsOpen, setCitationsOpen] = useState(false)

 // Reset state when dialog closes
 useEffect(() => {
 if (!open) {
 setPhase('idle')
 setResult(null)
 setEditTasks([])
 setError(null)
 setProgressIdx(0)
 setExpandedDesc(new Set())
 setCitationsOpen(false)
 }
 }, [open])

 // Animate progress messages during generation
 useEffect(() => {
 if (phase !== 'generating') return
 setProgressIdx(0)
 const interval = setInterval(() => {
 setProgressIdx((prev) => (prev < PROGRESS_MESSAGES.length - 1 ? prev + 1 : prev))
 }, 2000)
 return () => clearInterval(interval)
 }, [phase])

 const handleGenerate = useCallback(async () => {
 if (!token || !orgSlug) {
 setError('Missing authentication or organization context.')
 return
 }

 setPhase('generating')
 setError(null)

 try {
 const { ragUrl, apiKey } = detectApiUrls()
 const ragClient = new RagClient(ragUrl, apiKey, token)
 const bpeClient = new BpeClient(token)

 const genResult = await generateTasksForGoal(goal, orgSlug, ragClient, bpeClient)
 const tasksWithDates = spreadDueDates(genResult.tasks, goal.target_date)
 const finalResult: GenerationResult = { ...genResult, tasks: tasksWithDates }

 setResult(finalResult)
 setEditTasks(
 finalResult.tasks.map((t) => ({ ...t, selected: true })),
 )
 setPhase('review')
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Task generation failed.'
 setError(msg)
 setPhase('idle')
 }
 }, [token, orgSlug, goal])

 const handleSave = useCallback(async () => {
 const selected = editTasks.filter((t) => t.selected)
 if (selected.length === 0) {
 toast.error('No tasks selected')
 return
 }
 if (!user?.id || !currentOrg?.id || !token) {
 toast.error('Missing user or organization context')
 return
 }

 setPhase('saving')

 try {
 const { postgrestUrl, apiKey } = detectApiUrls()
 const pgClient = new PostgRestClient(postgrestUrl, apiKey)

 const taskBodies = selected.map((t) => ({
 task_number: 'T-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
 title: t.title,
 description: t.description,
 priority: t.priority,
 due_date: t.due_date || null,
 status: 'Assigned',
 goal_id: goal.id,
 organization_id: currentOrg.id,
 created_by: user.id,
 }))

 await pgClient.postMany('tasks', taskBodies, token)

 // Record feedback on BPE-sourced suggestions
 try {
 const bpeClient = new BpeClient(token)
 const seqFeedback = new Map<string, boolean>()
 for (const t of editTasks) {
 if (t.sequence_id && !seqFeedback.has(t.sequence_id)) {
 seqFeedback.set(t.sequence_id, t.selected)
 }
 }
 for (const [seqId, wasSelected] of seqFeedback) {
 await bpeClient.recordSequenceFeedback(seqId, wasSelected ? 'accepted' : 'rejected').catch(() => {})
 }
 } catch { /* non-critical */ }

 toast.success(`Created ${selected.length} task${selected.length > 1 ? 's' : ''} successfully`)
 onTasksCreated?.()
 onOpenChange(false)
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to create tasks'
 toast.error(msg)
 setPhase('review')
 }
 }, [editTasks, user, currentOrg, token, goal.id, onTasksCreated, onOpenChange])

 const updateTask = (index: number, updates: Partial<EditableTask>) => {
 setEditTasks((prev) => prev.map((t, i) => (i === index ? { ...t, ...updates } : t)))
 }

 const toggleDescription = (index: number) => {
 setExpandedDesc((prev) => {
 const next = new Set(prev)
 if (next.has(index)) next.delete(index)
 else next.add(index)
 return next
 })
 }

 const selectedCount = editTasks.filter((t) => t.selected).length

 const sourceIcon = (source: GeneratedTask['source']) => {
 switch (source) {
 case 'knowledge_base': return <FileText className="h-3 w-3" />
 case 'workflow': return <GitBranch className="h-3 w-3" />
 case 'ai': return <Brain className="h-3 w-3" />
 }
 }

 const sourceLabel = (source: GeneratedTask['source']) => {
 switch (source) {
 case 'knowledge_base': return 'KB'
 case 'workflow': return 'Workflow'
 case 'ai': return 'AI'
 }
 }

 const sourceVariant = (source: GeneratedTask['source']): 'default' | 'secondary' | 'outline' | 'destructive' => {
 switch (source) {
 case 'knowledge_base': return 'default'
 case 'workflow': return 'secondary'
 case 'ai': return 'outline'
 }
 }

 return (
 <Dialog open={open} onOpenChange={onOpenChange}>
 <DialogContent className="max-w-3xl max-h-[85vh] overflow-y-auto">
 <DialogHeader>
 <DialogTitle className="flex items-center gap-2">
 <Sparkles className="h-5 w-5 text-purple-500" />
 AI Task Generator
 </DialogTitle>
 </DialogHeader>

 {/* Phase: idle */}
 {phase === 'idle' && (
 <div className="space-y-4 py-4">
 <div className="rounded-lg border p-4 bg-muted/50">
 <h3 className="font-medium text-sm text-muted-foreground mb-1">Goal</h3>
 <p className="font-semibold">{goal.title}</p>
 {goal.description && (
 <p className="text-sm text-muted-foreground mt-1 line-clamp-3">
 {goal.description}
 </p>
 )}
 {goal.target_date && (
 <p className="text-xs text-muted-foreground mt-2">
 Target: {goal.target_date}
 </p>
 )}
 </div>

 {error && (
 <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
 {error}
 <Button variant="ghost" size="sm" className="ml-2" onClick={() => setError(null)}>
 Dismiss
 </Button>
 </div>
 )}

 <Button onClick={handleGenerate} className="w-full" size="lg">
 <Sparkles className="h-4 w-4 mr-2" />
 Generate Tasks
 </Button>
 </div>
 )}

 {/* Phase: generating */}
 {phase === 'generating' && (
 <div className="flex flex-col items-center justify-center py-12 space-y-4">
 <Loader2 className="h-8 w-8 animate-spin text-purple-500" />
 <div className="space-y-2 text-center">
 {PROGRESS_MESSAGES.map((msg, i) => (
 <p
 key={i}
 className={`text-sm transition-opacity duration-500 ${
 i <= progressIdx ? 'opacity-100' : 'opacity-30'
 } ${i === progressIdx ? 'font-medium' : ''}`}
 >
 {i < progressIdx && <Check className="h-3.5 w-3.5 inline mr-1 text-green-500" />}
 {i === progressIdx && <Loader2 className="h-3.5 w-3.5 inline mr-1 animate-spin" />}
 {msg}
 </p>
 ))}
 </div>
 </div>
 )}

 {/* Phase: review */}
 {phase === 'review' && result && (
 <div className="space-y-4 py-2">
 {/* Summary bar */}
 <div className="flex items-center justify-between rounded-lg border p-3 bg-muted/50">
 <div className="text-sm">
 Generated <span className="font-semibold">{result.tasks.length}</span> task{result.tasks.length !== 1 ? 's' : ''}
 {result.citations.length > 0 && (
 <> with <span className="font-semibold">{result.citations.length}</span> citation{result.citations.length !== 1 ? 's' : ''}</>
 )}
 {result.bpePatternCount > 0 && (
 <> from <span className="font-semibold">{result.bpePatternCount}</span> workflow pattern{result.bpePatternCount !== 1 ? 's' : ''}</>
 )}
 </div>
 </div>

 {/* Task list */}
 <div className="space-y-3 max-h-[40vh] overflow-y-auto pr-1">
 {editTasks.map((task, idx) => (
 <div
 key={idx}
 className={`rounded-lg border p-3 space-y-2 transition-opacity ${
 task.selected ? '' : 'opacity-50'
 }`}
 >
 {/* Row 1: checkbox + title + source badge */}
 <div className="flex items-center gap-2">
 <input
 type="checkbox"
 checked={task.selected}
 onChange={(e) => updateTask(idx, { selected: e.target.checked })}
 className="h-4 w-4 rounded border-gray-300"
 />
 <Input
 value={task.title}
 onChange={(e) => updateTask(idx, { title: e.target.value })}
 className="flex-1 h-8 text-sm font-medium"
 />
 <Badge variant={sourceVariant(task.source)} className="flex items-center gap-1 shrink-0">
 {sourceIcon(task.source)}
 {sourceLabel(task.source)}
 </Badge>
 </div>

 {/* Row 2: description (collapsible) */}
 <div>
 <button
 onClick={() => toggleDescription(idx)}
 className="text-xs text-muted-foreground hover:text-foreground transition-colors"
 >
 {expandedDesc.has(idx) ? 'Hide description' : 'Show description'}
 </button>
 {expandedDesc.has(idx) && (
 <Textarea
 value={task.description}
 onChange={(e) => updateTask(idx, { description: e.target.value })}
 rows={2}
 className="mt-1 text-sm"
 />
 )}
 </div>

 {/* Row 3: priority + due date */}
 <div className="flex items-center gap-3">
 <Select
 value={task.priority}
 onValueChange={(v) => updateTask(idx, { priority: v as 'Low' | 'Medium' | 'High' })}
 >
 <SelectTrigger className="w-28 h-8 text-xs">
 <SelectValue />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="Low">Low</SelectItem>
 <SelectItem value="Medium">Medium</SelectItem>
 <SelectItem value="High">High</SelectItem>
 </SelectContent>
 </Select>
 <Input
 type="date"
 value={task.due_date || ''}
 onChange={(e) => updateTask(idx, { due_date: e.target.value })}
 className="w-40 h-8 text-xs"
 />
 </div>
 </div>
 ))}
 </div>

 {/* Citations section (collapsible) */}
 {result.citations.length > 0 && (
 <div className="rounded-lg border">
 <button
 onClick={() => setCitationsOpen(!citationsOpen)}
 className="w-full flex items-center justify-between p-3 text-sm font-medium hover:bg-muted/50 transition-colors"
 >
 <span className="flex items-center gap-2">
 <FileText className="h-4 w-4" />
 Citations ({result.citations.length})
 </span>
 <span className="text-muted-foreground text-xs">
 {citationsOpen ? 'Hide' : 'Show'}
 </span>
 </button>
 {citationsOpen && (
 <div className="border-t p-3 space-y-2 max-h-40 overflow-y-auto">
 {result.citations.map((c, i) => (
 <div key={i} className="text-xs space-y-0.5">
 <p className="font-medium text-muted-foreground">{c.filename}</p>
 <p className="text-muted-foreground/80 line-clamp-2">{c.snippet}</p>
 </div>
 ))}
 </div>
 )}
 </div>
 )}

 {/* Action buttons */}
 <div className="flex justify-end gap-2 pt-2">
 <Button variant="ghost" onClick={() => onOpenChange(false)}>
 Cancel
 </Button>
 <Button variant="outline" onClick={handleGenerate}>
 <RefreshCw className="h-4 w-4 mr-1" />
 Regenerate
 </Button>
 <Button onClick={handleSave} disabled={selectedCount === 0}>
 <Check className="h-4 w-4 mr-1" />
 Create {selectedCount} Selected Task{selectedCount !== 1 ? 's' : ''}
 </Button>
 </div>
 </div>
 )}

 {/* Phase: saving */}
 {phase === 'saving' && (
 <div className="flex flex-col items-center justify-center py-12 space-y-3">
 <Loader2 className="h-8 w-8 animate-spin text-purple-500" />
 <p className="text-sm text-muted-foreground">Creating tasks...</p>
 </div>
 )}
 </DialogContent>
 </Dialog>
 )
}
