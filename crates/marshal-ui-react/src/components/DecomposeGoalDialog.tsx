import { useState, useEffect, useCallback } from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Loader2, Sparkles, Check, X, RefreshCw, ChevronRight, ChevronDown,
  Target, ListTodo, FileText, Brain,
} from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { RagClient } from '@/api/rag'
import { PostgRestClient } from '@/api/postgrest'
import { detectApiUrls } from '@/lib/config'
import {
  decomposeGoal, countTree,
  type DecomposedGoal, type DecomposedTask, type DecompositionResult,
} from '@/api/goalDecomposer'
import type { Goal } from '@/models/goal'

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  goal: Goal
  onCreated?: () => void
}

type Phase = 'idle' | 'generating' | 'review' | 'saving'

const PROGRESS_MESSAGES = [
  'Querying knowledge base for guidelines...',
  'Checking workflow patterns...',
  'Decomposing goal into sub-goals and tasks...',
]

export function DecomposeGoalDialog({ open, onOpenChange, goal, onCreated }: Props) {
  const token = useAuthStore((s) => s.token)
  const user = useAuthStore((s) => s.user)
  const currentOrg = useOrgStore((s) => s.currentOrg)
  const orgSlug = useOrgStore((s) => s.currentOrgSlug)

  const [phase, setPhase] = useState<Phase>('idle')
  const [result, setResult] = useState<DecompositionResult | null>(null)
  const [tree, setTree] = useState<DecomposedGoal | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [expandedNodes, setExpandedNodes] = useState<Set<string>>(new Set())
  const [citationsOpen, setCitationsOpen] = useState(false)
  const [savingProgress, setSavingProgress] = useState('')
  const [progressIdx, setProgressIdx] = useState(0)

  // Reset state when dialog closes
  useEffect(() => {
    if (!open) {
      setPhase('idle')
      setResult(null)
      setTree(null)
      setError(null)
      setExpandedNodes(new Set())
      setCitationsOpen(false)
      setSavingProgress('')
      setProgressIdx(0)
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

  // --- Tree path helpers ---

  function getNodeAtPath(root: DecomposedGoal, path: string): DecomposedGoal | null {
    if (path === '') return root
    const indices = path.split('.').map(Number)
    let node = root
    for (const idx of indices) {
      if (!node.sub_goals[idx]) return null
      node = node.sub_goals[idx]
    }
    return node
  }

  function updateNodeAtPath(
    root: DecomposedGoal,
    path: string,
    updater: (node: DecomposedGoal) => DecomposedGoal,
  ): DecomposedGoal {
    if (path === '') return updater({ ...root })
    const indices = path.split('.').map(Number)
    return updateNodeRecursive({ ...root }, indices, 0, updater)
  }

  function updateNodeRecursive(
    node: DecomposedGoal,
    indices: number[],
    depth: number,
    updater: (node: DecomposedGoal) => DecomposedGoal,
  ): DecomposedGoal {
    if (depth === indices.length) return updater(node)
    const idx = indices[depth]
    const newSubGoals = [...node.sub_goals]
    newSubGoals[idx] = updateNodeRecursive(
      { ...newSubGoals[idx] },
      indices, depth + 1, updater,
    )
    return { ...node, sub_goals: newSubGoals }
  }

  function setSelectedRecursive(node: DecomposedGoal, selected: boolean): DecomposedGoal {
    return {
      ...node,
      selected,
      tasks: node.tasks.map((t) => ({ ...t, selected })),
      sub_goals: node.sub_goals.map((sg) => setSelectedRecursive(sg, selected)),
    }
  }

  // Expand all nodes in a tree, collecting paths
  function collectAllPaths(node: DecomposedGoal, prefix: string): string[] {
    const paths: string[] = [prefix]
    node.sub_goals.forEach((_, i) => {
      const childPath = prefix === '' ? String(i) : `${prefix}.${i}`
      paths.push(...collectAllPaths(node.sub_goals[i], childPath))
    })
    return paths
  }

  // --- Toggle helpers ---

  const toggleGoalSelected = useCallback((path: string) => {
    setTree((prev) => {
      if (!prev) return prev
      const node = getNodeAtPath(prev, path)
      if (!node) return prev
      const newSelected = !node.selected
      return updateNodeAtPath(prev, path, (n) => setSelectedRecursive(n, newSelected))
    })
  }, [])

  const toggleTaskSelected = useCallback((goalPath: string, taskIndex: number) => {
    setTree((prev) => {
      if (!prev) return prev
      return updateNodeAtPath(prev, goalPath, (n) => ({
        ...n,
        tasks: n.tasks.map((t, i) => (i === taskIndex ? { ...t, selected: !t.selected } : t)),
      }))
    })
  }, [])

  const updateGoalTitle = useCallback((path: string, newTitle: string) => {
    setTree((prev) => {
      if (!prev) return prev
      return updateNodeAtPath(prev, path, (n) => ({ ...n, title: newTitle }))
    })
  }, [])

  const updateTaskTitle = useCallback((goalPath: string, taskIndex: number, newTitle: string) => {
    setTree((prev) => {
      if (!prev) return prev
      return updateNodeAtPath(prev, goalPath, (n) => ({
        ...n,
        tasks: n.tasks.map((t, i) => (i === taskIndex ? { ...t, title: newTitle } : t)),
      }))
    })
  }, [])

  const toggleExpanded = useCallback((path: string) => {
    setExpandedNodes((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }, [])

  // --- Generate ---

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

      const genResult = await decomposeGoal(goal, orgSlug, ragClient, bpeClient)
      setResult(genResult)

      const editableTree = structuredClone(genResult.tree)
      setTree(editableTree)

      // Expand all nodes by default
      const allPaths = collectAllPaths(editableTree, '')
      setExpandedNodes(new Set(allPaths))

      setPhase('review')
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Goal decomposition failed.'
      setError(msg)
      setPhase('idle')
    }
  }, [token, orgSlug, goal])

  // --- Save ---

  const handleSave = useCallback(async () => {
    if (!tree || !user?.id || !currentOrg?.id || !token) {
      toast.error('Missing user or organization context')
      return
    }

    setPhase('saving')
    setSavingProgress('Creating sub-goals...')

    try {
      const { postgrestUrl, apiKey } = detectApiUrls()
      const pgClient = new PostgRestClient(postgrestUrl, apiKey)

      async function createTreeNodes(node: DecomposedGoal, parentGoalId: string) {
        // Create tasks directly under this goal
        const selectedTasks = node.tasks.filter((t) => t.selected)
        if (selectedTasks.length > 0) {
          setSavingProgress(`Creating ${selectedTasks.length} tasks...`)
          const taskBodies = selectedTasks.map((t) => ({
            task_number: 'T-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
            title: t.title,
            description: t.description,
            priority: t.priority,
            status: 'Assigned',
            goal_id: parentGoalId,
            organization_id: currentOrg!.id,
            created_by: user!.id,
          }))
          await pgClient.postMany('tasks', taskBodies, token)
        }

        // Create selected sub-goals and recurse
        for (const sg of node.sub_goals) {
          if (!sg.selected) continue
          setSavingProgress(`Creating sub-goal: ${sg.title}...`)
          const created = await pgClient.post<{ id: string }>('goals', {
            title: sg.title,
            description: sg.description,
            status: 'not_started',
            parent_goal_id: parentGoalId,
            organization_id: currentOrg!.id,
            created_by: user!.id,
            target_date: goal.target_date,
          }, token)
          await createTreeNodes(sg, created.id)
        }
      }

      await createTreeNodes(tree, goal.id)

      const counts = countTree(tree)
      toast.success(`Created ${counts.goals} sub-goal${counts.goals !== 1 ? 's' : ''} and ${counts.tasks} task${counts.tasks !== 1 ? 's' : ''}`)
      onCreated?.()
      onOpenChange(false)
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to create goals and tasks'
      toast.error(msg)
      setPhase('review')
    }
  }, [tree, user, currentOrg, token, goal.id, goal.target_date, onCreated, onOpenChange])

  // --- Source helpers ---

  const sourceIcon = (source: DecomposedTask['source']) => {
    switch (source) {
      case 'knowledge_base': return <FileText className="h-3 w-3" />
      case 'workflow': return <Brain className="h-3 w-3" />
      case 'ai': return <Brain className="h-3 w-3" />
    }
  }

  const sourceLabel = (source: DecomposedTask['source']) => {
    switch (source) {
      case 'knowledge_base': return 'KB'
      case 'workflow': return 'Workflow'
      case 'ai': return 'AI'
    }
  }

  const sourceVariant = (source: DecomposedTask['source']): 'default' | 'secondary' | 'outline' => {
    switch (source) {
      case 'knowledge_base': return 'default'
      case 'workflow': return 'secondary'
      case 'ai': return 'outline'
    }
  }

  // --- Recursive tree renderer ---

  function renderGoalNode(node: DecomposedGoal, path: string, depth: number) {
    const isExpanded = expandedNodes.has(path)
    const hasChildren = node.sub_goals.length > 0 || node.tasks.length > 0

    return (
      <div key={path} className="space-y-1">
        {/* Goal row */}
        <div
          className={`flex items-center gap-2 rounded-md p-1.5 transition-opacity ${
            node.selected ? '' : 'opacity-50'
          }`}
          style={{ paddingLeft: `${depth * 24}px` }}
        >
          {/* Expand/collapse */}
          <button
            onClick={() => toggleExpanded(path)}
            className="shrink-0 p-0.5 hover:bg-muted rounded"
            disabled={!hasChildren}
          >
            {hasChildren ? (
              isExpanded
                ? <ChevronDown className="h-4 w-4 text-muted-foreground" />
                : <ChevronRight className="h-4 w-4 text-muted-foreground" />
            ) : (
              <span className="inline-block w-4" />
            )}
          </button>

          {/* Checkbox */}
          <input
            type="checkbox"
            checked={node.selected}
            onChange={() => toggleGoalSelected(path)}
            className="h-4 w-4 rounded border-gray-300 shrink-0"
          />

          {/* Icon */}
          <Target className="h-4 w-4 text-blue-500 shrink-0" />

          {/* Editable title */}
          <Input
            value={node.title}
            onChange={(e) => updateGoalTitle(path, e.target.value)}
            className={`flex-1 h-7 ${depth === 0 ? 'text-sm font-semibold' : depth === 1 ? 'text-sm font-medium' : 'text-xs font-medium'}`}
          />
        </div>

        {/* Expanded children */}
        {isExpanded && (
          <div>
            {/* Tasks under this goal */}
            {node.tasks.map((task, taskIdx) => (
              <div
                key={`${path}-t${taskIdx}`}
                className={`flex items-center gap-2 rounded-md p-1.5 transition-opacity ${
                  task.selected ? '' : 'opacity-50'
                }`}
                style={{ paddingLeft: `${(depth + 1) * 24 + 20}px` }}
              >
                <input
                  type="checkbox"
                  checked={task.selected}
                  onChange={() => toggleTaskSelected(path, taskIdx)}
                  className="h-3.5 w-3.5 rounded border-gray-300 shrink-0"
                />
                <ListTodo className="h-3.5 w-3.5 text-green-500 shrink-0" />
                <Input
                  value={task.title}
                  onChange={(e) => updateTaskTitle(path, taskIdx, e.target.value)}
                  className="flex-1 h-6 text-xs"
                />
                <Select
                  value={task.priority}
                  onValueChange={(v) => {
                    setTree((prev) => {
                      if (!prev) return prev
                      return updateNodeAtPath(prev, path, (n) => ({
                        ...n,
                        tasks: n.tasks.map((t, i) =>
                          i === taskIdx ? { ...t, priority: v as 'Low' | 'Medium' | 'High' } : t,
                        ),
                      }))
                    })
                  }}
                >
                  <SelectTrigger className="w-20 h-6 text-xs shrink-0">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="Low">Low</SelectItem>
                    <SelectItem value="Medium">Medium</SelectItem>
                    <SelectItem value="High">High</SelectItem>
                  </SelectContent>
                </Select>
                <Badge variant={sourceVariant(task.source)} className="flex items-center gap-1 shrink-0 text-[10px] px-1.5 py-0">
                  {sourceIcon(task.source)}
                  {sourceLabel(task.source)}
                </Badge>
              </div>
            ))}

            {/* Sub-goals (recursive) */}
            {node.sub_goals.map((sg, idx) => {
              const childPath = path === '' ? String(idx) : `${path}.${idx}`
              return renderGoalNode(sg, childPath, depth + 1)
            })}
          </div>
        )}
      </div>
    )
  }

  // --- Computed ---

  const counts = tree ? countTree(tree) : { goals: 0, tasks: 0, levels: 0 }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-purple-500" />
            AI Goal Decomposition
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
              <Target className="h-4 w-4 mr-2" />
              Decompose Goal
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
        {phase === 'review' && result && tree && (
          <div className="space-y-4 py-2">
            {/* Summary bar */}
            <div className="flex items-center justify-between rounded-lg border p-3 bg-muted/50">
              <div className="text-sm">
                Generated <span className="font-semibold">{counts.goals}</span> sub-goal{counts.goals !== 1 ? 's' : ''},
                {' '}<span className="font-semibold">{counts.tasks}</span> task{counts.tasks !== 1 ? 's' : ''}
                {' '}across <span className="font-semibold">{counts.levels}</span> level{counts.levels !== 1 ? 's' : ''}
                {result.citations.length > 0 && (
                  <> with <span className="font-semibold">{result.citations.length}</span> citation{result.citations.length !== 1 ? 's' : ''}</>
                )}
              </div>
            </div>

            {/* Tree view */}
            <div className="space-y-1 max-h-[45vh] overflow-y-auto pr-1 border rounded-lg p-2">
              {renderGoalNode(tree, '', 0)}
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
              <Button onClick={handleSave} disabled={counts.goals === 0 && counts.tasks === 0}>
                <Check className="h-4 w-4 mr-1" />
                Create All Selected
              </Button>
            </div>
          </div>
        )}

        {/* Phase: saving */}
        {phase === 'saving' && (
          <div className="flex flex-col items-center justify-center py-12 space-y-3">
            <Loader2 className="h-8 w-8 animate-spin text-purple-500" />
            <p className="text-sm text-muted-foreground">{savingProgress}</p>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
