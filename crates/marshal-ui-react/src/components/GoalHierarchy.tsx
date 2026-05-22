import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { ChevronRight, ChevronDown, Target, CheckSquare, Plus, Edit, Trash2 } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
 AlertDialog,
 AlertDialogAction,
 AlertDialogCancel,
 AlertDialogContent,
 AlertDialogDescription,
 AlertDialogFooter,
 AlertDialogHeader,
 AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import { format } from 'date-fns'
import { calculateGoalProgress } from '@/lib/goalProgress'
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

type GoalHierarchyProps = {
 goals: Goal[]
 tasks: TaskItem[]
 onEditGoal: (goal: Goal) => void
 onAddSubGoal: (parentId: string) => void
 onAddTask: (goalId: string) => void
 onRefresh: () => void
 currentUserId: string
 isAdmin: boolean
}

const getStatusColor = (status: string | null) => {
 switch (status) {
 case 'completed':
 return 'bg-emerald-100 text-emerald-700'
 case 'in_progress':
 return 'bg-amber-100 text-amber-700'
 case 'on_hold':
 return 'bg-gray-100 text-gray-600'
 default:
 return 'bg-gray-100 text-gray-600'
 }
}

const getStatusLabel = (status: string | null) => {
 switch (status) {
 case 'not_started': return 'Not Started'
 case 'in_progress': return 'In Progress'
 case 'completed': return 'Completed'
 case 'on_hold': return 'On Hold'
 default: return status || 'Not Started'
 }
}

const getPriorityColor = (priority: string | null) => {
 switch ((priority || '').toLowerCase()) {
 case 'high': return 'bg-red-100 text-red-700'
 case 'medium': return 'bg-amber-100 text-amber-700'
 case 'low': return 'bg-emerald-100 text-emerald-700'
 default: return 'bg-gray-100 text-gray-600'
 }
}

function GoalItem({
 goal,
 goals,
 tasks,
 level,
 onEditGoal,
 onAddSubGoal,
 onAddTask,
 onRefresh,
 currentUserId,
 isAdmin,
}: {
 goal: Goal
 goals: Goal[]
 tasks: TaskItem[]
 level: number
 onEditGoal: (goal: Goal) => void
 onAddSubGoal: (parentId: string) => void
 onAddTask: (goalId: string) => void
 onRefresh: () => void
 currentUserId: string
 isAdmin: boolean
}) {
 const navigate = useNavigate()
 const token = useAuthStore((s) => s.token)
 const { postgrestUrl, apiKey } = detectApiUrls()
 const [expanded, setExpanded] = useState(true)
 const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)

 const childGoals = goals.filter((g) => g.parent_goal_id === goal.id)
 const goalTasks = tasks.filter((t) => t.goal_id === goal.id)
 const hasChildren = childGoals.length > 0 || goalTasks.length > 0

 const canEdit = goal.created_by === currentUserId || isAdmin
 const progress = calculateGoalProgress(goal.id, goals, tasks)

 const handleDelete = async () => {
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().eq('id', goal.id).build()
 await client.delete('goals', qs, token)
 toast.success('Goal deleted successfully')
 onRefresh()
 } catch (error: unknown) {
 const msg = error instanceof Error ? error.message : 'Failed to delete goal'
 console.error('Error deleting goal:', error)
 toast.error(msg)
 }
 setDeleteDialogOpen(false)
 }

 return (
 <div className="border-l-2 border-muted ml-4 first:ml-0 first:border-l-0">
 <div
 className={`flex items-center gap-2 p-3 hover:bg-muted/50 rounded-r-lg ${level === 0 ? 'ml-0' : ''}`}
 style={{ paddingLeft: `${level * 16 + 12}px` }}
 >
 <button
 onClick={() => setExpanded(!expanded)}
 className="p-1 hover:bg-muted rounded"
 disabled={!hasChildren}
 >
 {hasChildren ? (
 expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />
 ) : (
 <div className="w-4" />
 )}
 </button>

 <div
 className="flex items-center gap-2 flex-1 min-w-0 cursor-pointer"
 onClick={() => navigate(`/goals/${goal.id}`)}
 >
 <Target className="h-5 w-5 text-primary shrink-0" />

 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2">
 <span className="font-medium truncate hover:text-primary">{goal.title}</span>
 <Badge variant="outline" className={getStatusColor(goal.status)}>
 {getStatusLabel(goal.status)}
 </Badge>
 </div>
 <div className="flex items-center gap-2 mt-1">
 <Progress value={progress} className="h-2 flex-1 max-w-[200px]" />
 <span className="text-xs text-muted-foreground w-8">{progress}%</span>
 </div>
 </div>

 {goal.target_date && (
 <span className="text-sm text-muted-foreground">
 {format(new Date(goal.target_date), 'MMM d, yyyy')}
 </span>
 )}
 </div>

 <div className="flex items-center gap-1">
 <Tooltip>
 <TooltipTrigger asChild>
 <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => onAddSubGoal(goal.id)}>
 <Plus className="h-4 w-4" />
 </Button>
 </TooltipTrigger>
 <TooltipContent>Add sub-goal</TooltipContent>
 </Tooltip>

 <Tooltip>
 <TooltipTrigger asChild>
 <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => onAddTask(goal.id)}>
 <CheckSquare className="h-4 w-4" />
 </Button>
 </TooltipTrigger>
 <TooltipContent>Add task</TooltipContent>
 </Tooltip>

 {canEdit && (
 <>
 <Tooltip>
 <TooltipTrigger asChild>
 <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => onEditGoal(goal)}>
 <Edit className="h-4 w-4" />
 </Button>
 </TooltipTrigger>
 <TooltipContent>Edit goal</TooltipContent>
 </Tooltip>

 <Tooltip>
 <TooltipTrigger asChild>
 <Button
 variant="ghost"
 size="icon"
 className="h-8 w-8 text-destructive hover:text-destructive"
 onClick={() => setDeleteDialogOpen(true)}
 >
 <Trash2 className="h-4 w-4" />
 </Button>
 </TooltipTrigger>
 <TooltipContent>Delete goal</TooltipContent>
 </Tooltip>
 </>
 )}
 </div>
 </div>

 {expanded && hasChildren && (
 <div>
 {childGoals.map((childGoal) => (
 <GoalItem
 key={childGoal.id}
 goal={childGoal}
 goals={goals}
 tasks={tasks}
 level={level + 1}
 onEditGoal={onEditGoal}
 onAddSubGoal={onAddSubGoal}
 onAddTask={onAddTask}
 onRefresh={onRefresh}
 currentUserId={currentUserId}
 isAdmin={isAdmin}
 />
 ))}

 {goalTasks.map((task) => (
 <div
 key={task.id}
 className="flex items-center gap-2 p-3 hover:bg-muted/50 rounded-r-lg cursor-pointer"
 style={{ paddingLeft: `${(level + 1) * 16 + 28}px` }}
 onClick={() => navigate(`/tasks/${task.id}`)}
 >
 <CheckSquare className="h-4 w-4 text-muted-foreground" />
 {task.task_number && <span className="text-sm font-medium text-primary">{task.task_number}</span>}
 <span className="text-sm truncate flex-1">{task.title}</span>
 <Badge variant="outline" className={getPriorityColor(task.priority)}>
 {task.priority || 'None'}
 </Badge>
 {task.due_date && (
 <span className="text-sm text-muted-foreground">
 {format(new Date(task.due_date), 'MMM d')}
 </span>
 )}
 </div>
 ))}
 </div>
 )}

 <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>Delete Goal</AlertDialogTitle>
 <AlertDialogDescription>
 Are you sure you want to delete &quot;{goal.title}&quot;? This will also delete all sub-goals. Tasks will be unlinked but not deleted.
 </AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel>Cancel</AlertDialogCancel>
 <AlertDialogAction onClick={handleDelete} className="bg-destructive text-destructive-foreground">
 Delete
 </AlertDialogAction>
 </AlertDialogFooter>
 </AlertDialogContent>
 </AlertDialog>
 </div>
 )
}

export function GoalHierarchy({
 goals,
 tasks,
 onEditGoal,
 onAddSubGoal,
 onAddTask,
 onRefresh,
 currentUserId,
 isAdmin,
}: GoalHierarchyProps) {
 const topLevelGoals = goals.filter((g) => !g.parent_goal_id)

 if (topLevelGoals.length === 0) {
 return (
 <div className="text-center py-12 text-muted-foreground">
 <Target className="h-12 w-12 mx-auto mb-4 opacity-50" />
 <p>No goals yet. Create your first goal to get started.</p>
 </div>
 )
 }

 return (
 <div className="space-y-2">
 {topLevelGoals.map((goal) => (
 <GoalItem
 key={goal.id}
 goal={goal}
 goals={goals}
 tasks={tasks}
 level={0}
 onEditGoal={onEditGoal}
 onAddSubGoal={onAddSubGoal}
 onAddTask={onAddTask}
 onRefresh={onRefresh}
 currentUserId={currentUserId}
 isAdmin={isAdmin}
 />
 ))}
 </div>
 )
}
