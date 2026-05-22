import { useState, useEffect, useCallback } from 'react'
import { useParams, Link } from 'react-router-dom'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Separator } from '@/components/ui/separator'
import { ArrowLeft, Loader2, Target, Calendar, ListTodo, GitBranch, Pencil, Plus, Sparkles, CheckCircle2, Brain, Network } from 'lucide-react'
import { GoalDialog } from '@/components/GoalDialog'
import { TaskDialog } from '@/components/TaskDialog'
import { GenerateTasksDialog } from '@/components/GenerateTasksDialog'
import { DecomposeGoalDialog } from '@/components/DecomposeGoalDialog'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import type { Goal } from '@/models/goal'
import type { Task } from '@/models/task'

function statusVariant(status: string): 'default' | 'secondary' | 'destructive' | 'outline' {
 switch (status?.toLowerCase()) {
 case 'completed':
 return 'default'
 case 'in_progress':
 case 'in progress':
 return 'secondary'
 case 'on_hold':
 case 'on hold':
 return 'outline'
 case 'not_started':
 case 'not started':
 return 'destructive'
 default:
 return 'outline'
 }
}

function formatStatus(status: string): string {
 return status.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

function inferGoalCategory(title: string): string {
 const lower = title.toLowerCase()
 if (lower.includes('onboard')) return 'onboarding'
 if (lower.includes('hire') || lower.includes('recruit')) return 'hiring'
 if (lower.includes('train')) return 'training'
 if (lower.includes('review') || lower.includes('audit')) return 'review'
 if (lower.includes('deploy') || lower.includes('launch')) return 'deployment'
 if (lower.includes('fix') || lower.includes('bug')) return 'bug_fix'
 if (lower.includes('improve') || lower.includes('optimize')) return 'improvement'
 if (lower.includes('compliance') || lower.includes('policy')) return 'compliance'
 if (lower.includes('safety') || lower.includes('incident')) return 'safety'
 return 'general'
}

export function GoalDetailPage() {
 const { id } = useParams<{ id: string }>()
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [goal, setGoal] = useState<Goal | null>(null)
 const [childGoals, setChildGoals] = useState<Goal[]>([])
 const [tasks, setTasks] = useState<Task[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [editOpen, setEditOpen] = useState(false)
 const [taskDialogOpen, setTaskDialogOpen] = useState(false)
 const [generateOpen, setGenerateOpen] = useState(false)
 const [decomposeOpen, setDecomposeOpen] = useState(false)
 const [learningWorkflow, setLearningWorkflow] = useState(false)
 const [workflowLearned, setWorkflowLearned] = useState(false)

 const fetchGoal = useCallback(async () => {
 if (!id) return
 setLoading(true)
 setError(null)

 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)

 const goalQs = new QueryBuilder().select('*').eq('id', id).build()
 const fetchedGoal = await client.getOne<Goal>('goals', goalQs, token)
 setGoal(fetchedGoal)

 const childrenQs = new QueryBuilder()
 .eq('parent_goal_id', id)
 .order('created_at', false)
 .build()
 const fetchedChildren = await client.get<Goal>('goals', childrenQs, token)
 setChildGoals(fetchedChildren)

 const tasksQs = new QueryBuilder()
 .eq('goal_id', id)
 .order('created_at', false)
 .build()
 const fetchedTasks = await client.get<Task>('tasks', tasksQs, token)
 setTasks(fetchedTasks)
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to load goal'
 setError(msg)
 } finally {
 setLoading(false)
 }
 }, [id, postgrestUrl, apiKey, token])

 useEffect(() => {
 fetchGoal()
 }, [fetchGoal])

 const handleLearnWorkflow = async () => {
 if (!token || !orgSlug || !goal || tasks.length === 0) return
 setLearningWorkflow(true)
 try {
 const bpeClient = new BpeClient(token)
 const category = inferGoalCategory(goal.title)
 await bpeClient.learnFromGoal({
 organization_id: orgSlug,
 goal_id: goal.id,
 goal_title: goal.title,
 task_category: category,
 tasks: tasks.map((t, i) => ({
 title: t.title,
 description: t.description ?? undefined,
 status: t.status,
 priority: t.priority ?? undefined,
 sequence_order: i + 1,
 })),
 })
 setWorkflowLearned(true)
 toast.success('Workflow learned! It will improve future task generation for similar goals.')

 // Auto-update goal to completed if not already
 if (goal.status !== 'completed') {
 try {
 const pgClient = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().eq('id', goal.id).build()
 await pgClient.patch('goals', qs, { status: 'completed', progress: 1 }, token)
 fetchGoal()
 } catch { /* non-critical */ }
 }
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to learn workflow')
 } finally {
 setLearningWorkflow(false)
 }
 }

 if (loading) {
 return (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
 <span className="ml-2 text-sm text-muted-foreground">Loading goal...</span>
 </div>
 )
 }

 if (error) {
 return (
 <Card className="border-destructive">
 <CardContent className="pt-6">
 <p className="text-sm text-destructive">{error}</p>
 </CardContent>
 </Card>
 )
 }

 if (!goal) {
 return (
 <div className="p-3 text-sm text-muted-foreground">Goal not found.</div>
 )
 }

 const progressPercent = goal.progress != null ? Math.round(goal.progress * 100) : null

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <Button variant="ghost" size="sm" asChild>
 <Link to="/goals">
 <ArrowLeft className="mr-1 h-4 w-4" />
 Back to goals
 </Link>
 </Button>
 <Button variant="outline" size="sm" onClick={() => setEditOpen(true)}>
 <Pencil className="mr-1 h-4 w-4" />
 Edit
 </Button>
 </div>

 <div className="flex items-start justify-between gap-4">
 <div className="space-y-1">
 <h1 className="text-2xl font-bold text-foreground">{goal.title}</h1>
 {goal.description && (
 <p className="text-muted-foreground">{goal.description}</p>
 )}
 </div>
 <Badge variant={statusVariant(goal.status)}>
 {formatStatus(goal.status)}
 </Badge>
 </div>

 <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
 <Target className="h-3.5 w-3.5" />
 Progress
 </div>
 <div className="mt-2 space-y-2">
 <span className="block text-lg font-bold text-foreground">
 {progressPercent != null ? `${progressPercent}%` : '--'}
 </span>
 {progressPercent != null && (
 <Progress value={progressPercent} className="h-2" />
 )}
 </div>
 </CardContent>
 </Card>

 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
 <Calendar className="h-3.5 w-3.5" />
 Target Date
 </div>
 <span className="mt-2 block text-lg font-bold text-foreground">
 {goal.target_date ?? 'No target date'}
 </span>
 </CardContent>
 </Card>

 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
 <GitBranch className="h-3.5 w-3.5" />
 Sub-Goals
 </div>
 <span className="mt-2 block text-lg font-bold text-foreground">
 {childGoals.length}
 </span>
 </CardContent>
 </Card>

 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
 <ListTodo className="h-3.5 w-3.5" />
 Tasks
 </div>
 <span className="mt-2 block text-lg font-bold text-foreground">
 {tasks.length}
 </span>
 </CardContent>
 </Card>
 </div>

 {childGoals.length > 0 && (
 <>
 <Separator />
 <div className="space-y-3">
 <h2 className="text-lg font-semibold text-foreground">Child Goals</h2>
 <div className="grid gap-3 sm:grid-cols-2">
 {childGoals.map((cg) => (
 <Card key={cg.id} className="hover:bg-muted/50 transition-colors">
 <CardContent className="flex items-center justify-between py-3 px-4">
 <Link
 to={`/goals/${cg.id}`}
 className="text-sm font-medium text-primary hover:underline"
 >
 {cg.title}
 </Link>
 <Badge variant={statusVariant(cg.status)} className="text-xs">
 {formatStatus(cg.status)}
 </Badge>
 </CardContent>
 </Card>
 ))}
 </div>
 </div>
 </>
 )}

 <Separator />
 <div className="space-y-3">
 <div className="flex items-center justify-between">
 <h2 className="text-lg font-semibold text-foreground">Tasks</h2>
 <div className="flex items-center gap-2">
 <Button variant="outline" size="sm" onClick={() => setTaskDialogOpen(true)}>
 <Plus className="mr-1 h-4 w-4" />
 Add Task
 </Button>
 <Button variant="outline" size="sm" onClick={() => setGenerateOpen(true)}>
 <Sparkles className="mr-1 h-4 w-4" />
 Generate Tasks
 </Button>
 <Button size="sm" onClick={() => setDecomposeOpen(true)}>
 <Network className="mr-1 h-4 w-4" />
 Decompose Goal
 </Button>
 </div>
 </div>
 {tasks.length === 0 ? (
 <Card>
 <CardContent className="py-8 text-center text-sm text-muted-foreground">
 No tasks yet. Add tasks manually or generate them with AI.
 </CardContent>
 </Card>
 ) : (
 <div className="grid gap-3 sm:grid-cols-2">
 {tasks.map((t) => (
 <Card key={t.id} className="hover:bg-muted/50 transition-colors">
 <CardContent className="flex items-center justify-between py-3 px-4">
 <Link
 to={`/tasks/${t.id}`}
 className="text-sm font-medium text-primary hover:underline"
 >
 {t.title}
 </Link>
 <Badge variant={statusVariant(t.status)} className="text-xs">
 {formatStatus(t.status)}
 </Badge>
 </CardContent>
 </Card>
 ))}
 </div>
 )}

 {/* Goal Completion — Learn Workflow */}
 {tasks.length > 0 && tasks.every((t) => t.status?.toLowerCase() === 'completed') && !workflowLearned && (
 <Card className="border-green-300 bg-green-50">
 <CardContent className="py-4 flex items-center justify-between gap-4">
 <div className="flex items-center gap-3">
 <CheckCircle2 className="h-6 w-6 text-green-600 flex-shrink-0" />
 <div>
 <p className="font-semibold text-green-800">All tasks completed!</p>
 <p className="text-sm text-green-700">Learn this workflow so the system can suggest similar tasks for future goals.</p>
 </div>
 </div>
 <Button
 size="sm"
 onClick={handleLearnWorkflow}
 disabled={learningWorkflow}
 className="flex-shrink-0"
 >
 {learningWorkflow ? <Loader2 className="h-4 w-4 mr-1 animate-spin" /> : <Brain className="h-4 w-4 mr-1" />}
 Learn Workflow
 </Button>
 </CardContent>
 </Card>
 )}
 {workflowLearned && (
 <Card className="border-green-300 bg-green-50">
 <CardContent className="py-3 flex items-center gap-3">
 <CheckCircle2 className="h-5 w-5 text-green-600" />
 <p className="text-sm text-green-800">Workflow learned! It will be suggested for similar future goals.</p>
 </CardContent>
 </Card>
 )}
 </div>

 <GoalDialog
 open={editOpen}
 onOpenChange={setEditOpen}
 onGoalSaved={fetchGoal}
 editingGoal={goal}
 />

 <TaskDialog
 open={taskDialogOpen}
 onOpenChange={setTaskDialogOpen}
 onTaskCreated={fetchGoal}
 defaultGoalId={id}
 />

 {goal && (
 <>
 <GenerateTasksDialog
 open={generateOpen}
 onOpenChange={setGenerateOpen}
 goal={goal}
 onTasksCreated={fetchGoal}
 />
 <DecomposeGoalDialog
 open={decomposeOpen}
 onOpenChange={setDecomposeOpen}
 goal={goal}
 onCreated={fetchGoal}
 />
 </>
 )}
 </div>
 )
}
