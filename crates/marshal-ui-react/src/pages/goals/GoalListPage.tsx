import { useState, useEffect } from 'react'
import { Plus, Target, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { GoalDialog } from '@/components/GoalDialog'
import { GoalHierarchy } from '@/components/GoalHierarchy'
import { TaskDialog } from '@/components/TaskDialog'
import { TooltipProvider } from '@/components/ui/tooltip'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import type { Goal } from '@/models/goal'

type TaskItem = {
 id: string
 task_number: string | null
 title: string
 due_date: string | null
 priority: string | null
 status: string
 goal_id: string | null
}

export function GoalListPage() {
 const user = useAuthStore((s) => s.user)
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [goals, setGoals] = useState<Goal[]>([])
 const [tasks, setTasks] = useState<TaskItem[]>([])
 const [loading, setLoading] = useState(true)

 // Dialog states
 const [goalDialogOpen, setGoalDialogOpen] = useState(false)
 const [taskDialogOpen, setTaskDialogOpen] = useState(false)
 const [editingGoal, setEditingGoal] = useState<Goal | null>(null)
 const [parentGoalId, setParentGoalId] = useState<string | null>(null)
 const [taskGoalId, setTaskGoalId] = useState<string | null>(null)

 const orgId = currentOrg?.id ?? ''
 const isAdmin = user?.is_platform_admin ?? false

 useEffect(() => {
 if (orgId) loadData()
 }, [orgId])

 const loadData = async () => {
 if (!orgId) return
 setLoading(true)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)

 const [goalsData, tasksData] = await Promise.all([
 client.get<Goal>(
 'goals',
 new QueryBuilder()
 .select('id,organization_id,title,description,status,progress,target_date,parent_goal_id,created_by,created_at,updated_at')
 .eq('organization_id', orgId)
 .order('created_at', false)
 .build(),
 token,
 ),
 client.get<TaskItem>(
 'tasks',
 new QueryBuilder()
 .select('id,task_number,title,due_date,priority,status,goal_id')
 .eq('organization_id', orgId)
 .build(),
 token,
 ),
 ])

 // Only keep tasks linked to goals
 const goalIds = new Set(goalsData.map((g) => g.id))
 const linkedTasks = tasksData.filter((t) => t.goal_id && goalIds.has(t.goal_id))

 setGoals(goalsData)
 setTasks(linkedTasks)
 } catch (error) {
 console.error('Error loading goals:', error)
 } finally {
 setLoading(false)
 }
 }

 const handleCreateGoal = () => {
 setEditingGoal(null)
 setParentGoalId(null)
 setGoalDialogOpen(true)
 }

 const handleEditGoal = (goal: Goal) => {
 setEditingGoal(goal)
 setParentGoalId(null)
 setGoalDialogOpen(true)
 }

 const handleAddSubGoal = (parentId: string) => {
 setEditingGoal(null)
 setParentGoalId(parentId)
 setGoalDialogOpen(true)
 }

 const handleAddTask = (goalId: string) => {
 setTaskGoalId(goalId)
 setTaskDialogOpen(true)
 }

 const handleGoalSaved = () => loadData()

 const handleTaskCreated = () => {
 loadData()
 setTaskDialogOpen(false)
 setTaskGoalId(null)
 }

 // Stats
 const totalGoals = goals.length
 const completedGoals = goals.filter((g) => g.status === 'completed').length
 const inProgressGoals = goals.filter((g) => g.status === 'in_progress').length
 const linkedTasks = tasks.length

 return (
 <TooltipProvider>
 <div>
 <div className="flex justify-between items-center mb-6">
 <div className="flex items-center gap-3">
 <Target className="h-8 w-8 text-primary" />
 <h1 className="text-3xl font-bold">Goals</h1>
 </div>
 <Button onClick={handleCreateGoal}>
 <Plus className="mr-2 h-4 w-4" />
 Goal
 </Button>
 </div>

 {/* Stats Cards */}
 <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
 <Card>
 <CardHeader className="pb-2">
 <CardTitle className="text-sm font-medium text-muted-foreground">Total Goals</CardTitle>
 </CardHeader>
 <CardContent>
 <p className="text-2xl font-bold">{totalGoals}</p>
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="pb-2">
 <CardTitle className="text-sm font-medium text-muted-foreground">In Progress</CardTitle>
 </CardHeader>
 <CardContent>
 <p className="text-2xl font-bold text-amber-600">{inProgressGoals}</p>
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="pb-2">
 <CardTitle className="text-sm font-medium text-muted-foreground">Completed</CardTitle>
 </CardHeader>
 <CardContent>
 <p className="text-2xl font-bold text-emerald-600">{completedGoals}</p>
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="pb-2">
 <CardTitle className="text-sm font-medium text-muted-foreground">Linked Tasks</CardTitle>
 </CardHeader>
 <CardContent>
 <p className="text-2xl font-bold text-primary">{linkedTasks}</p>
 </CardContent>
 </Card>
 </div>

 {/* Goals Hierarchy */}
 <Card>
 <CardHeader>
 <CardTitle>Goal Hierarchy</CardTitle>
 </CardHeader>
 <CardContent>
 {loading ? (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-8 w-8 animate-spin text-primary" />
 <span className="ml-2 text-muted-foreground">Loading goals...</span>
 </div>
 ) : (
 <GoalHierarchy
 goals={goals}
 tasks={tasks}
 onEditGoal={handleEditGoal}
 onAddSubGoal={handleAddSubGoal}
 onAddTask={handleAddTask}
 onRefresh={loadData}
 currentUserId={user?.id || ''}
 isAdmin={isAdmin}
 />
 )}
 </CardContent>
 </Card>

 {/* Goal Dialog */}
 <GoalDialog
 open={goalDialogOpen}
 onOpenChange={setGoalDialogOpen}
 onGoalSaved={handleGoalSaved}
 editingGoal={editingGoal}
 parentGoalId={parentGoalId}
 allGoals={goals}
 />

 {/* Task Dialog */}
 <TaskDialog
 open={taskDialogOpen}
 onOpenChange={setTaskDialogOpen}
 onTaskCreated={handleTaskCreated}
 defaultGoalId={taskGoalId || undefined}
 />
 </div>
 </TooltipProvider>
 )
}
