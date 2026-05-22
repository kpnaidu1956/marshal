import { useState, useEffect, useCallback } from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'

interface OrgUser {
 id: string
 first_name: string
 last_name: string
 username: string
}

interface OrgGoal {
 id: string
 title: string
}

interface TaskDialogProps {
 open: boolean
 onOpenChange: (open: boolean) => void
 onTaskCreated?: () => void
 taskId?: string
 defaultGoalId?: string
}

export function TaskDialog({ open, onOpenChange, onTaskCreated, taskId, defaultGoalId }: TaskDialogProps) {
 const user = useAuthStore((s) => s.user)
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [title, setTitle] = useState('')
 const [description, setDescription] = useState('')
 const [dueDate, setDueDate] = useState('')
 const [priority, setPriority] = useState('')
 const [assignedUser, setAssignedUser] = useState('')
 const [goalId, setGoalId] = useState<string>(defaultGoalId || '')
 const [users, setUsers] = useState<OrgUser[]>([])
 const [goals, setGoals] = useState<OrgGoal[]>([])
 const [isSubmitting, setIsSubmitting] = useState(false)

 const loadUsers = useCallback(async (cancelled: boolean) => {
 if (!currentOrg?.id) return
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder()
 .select('id,first_name,last_name,username')
 .eq('organization_id', currentOrg.id)
 .order('first_name', true)
 .build()
 const data = await client.get<OrgUser>('users', qs, token)
 if (!cancelled) setUsers(data)
 } catch (err) {
 console.error('Error loading users:', err)
 }
 }, [currentOrg?.id, postgrestUrl, apiKey, token])

 const loadGoals = useCallback(async (cancelled: boolean) => {
 if (!currentOrg?.id) return
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder()
 .select('id,title')
 .eq('organization_id', currentOrg.id)
 .order('title', true)
 .build()
 const data = await client.get<OrgGoal>('goals', qs, token)
 if (!cancelled) setGoals(data)
 } catch (err) {
 console.error('Error loading goals:', err)
 }
 }, [currentOrg?.id, postgrestUrl, apiKey, token])

 const loadTaskDetails = useCallback(async (cancelled: boolean) => {
 if (!taskId) return
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder()
 .select('title,description,due_date,priority,assigned_to,goal_id')
 .eq('id', taskId)
 .build()
 const data = await client.getOne<{
 title: string
 description: string
 due_date: string
 priority: string
 assigned_to: string | null
 goal_id: string | null
 }>('tasks', qs, token)
 if (cancelled) return
 setTitle(data.title)
 setDescription(data.description || '')
 setDueDate(data.due_date ? data.due_date.split('T')[0] : '')
 setPriority(data.priority || '')
 setAssignedUser(data.assigned_to || '')
 setGoalId(data.goal_id || '')
 } catch (err) {
 console.error('Error loading task:', err)
 toast.error('Failed to load task details')
 }
 }, [taskId, postgrestUrl, apiKey, token])

 useEffect(() => {
 let cancelled = false
 if (open) {
 loadUsers(cancelled)
 loadGoals(cancelled)
 if (taskId) loadTaskDetails(cancelled)
 else setGoalId(defaultGoalId || '')
 } else {
 resetForm()
 }
 return () => { cancelled = true }
 }, [open, taskId, loadUsers, loadTaskDetails])

 const handleSubmit = async () => {
 if (!title.trim()) { toast.error('Title is required'); return }
 if (!description.trim()) { toast.error('Description is required'); return }
 if (!dueDate) { toast.error('Due date is required'); return }
 if (!priority) { toast.error('Priority is required'); return }
 if (!user?.id) { toast.error('Unable to identify current user'); return }
 if (!currentOrg?.id) { toast.error('No organization selected'); return }

 setIsSubmitting(true)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)

 if (taskId) {
 const qs = new QueryBuilder().eq('id', taskId).build()
 await client.patch('tasks', qs, {
 title,
 description,
 due_date: dueDate,
 priority,
 assigned_to: assignedUser || user.id,
 goal_id: goalId || null,
 }, token)
 toast.success('Task updated successfully')
 } else {
 const taskNumber = 'T-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6)
 await client.post('tasks', {
 task_number: taskNumber,
 title,
 description,
 due_date: dueDate,
 priority,
 status: 'Assigned',
 assigned_to: assignedUser || user.id,
 created_by: user.id,
 goal_id: goalId || null,
 organization_id: currentOrg?.id || null,
 }, token)
 toast.success('Task created successfully')
 }

 resetForm()
 onOpenChange(false)
 onTaskCreated?.()
 } catch (error: unknown) {
 const msg = error instanceof Error ? error.message : `Failed to ${taskId ? 'update' : 'create'} task`
 console.error(`Error ${taskId ? 'updating' : 'creating'} task:`, error)
 toast.error(msg)
 } finally {
 setIsSubmitting(false)
 }
 }

 const resetForm = () => {
 setTitle('')
 setDescription('')
 setDueDate('')
 setPriority('')
 setAssignedUser('')
 setGoalId(defaultGoalId || '')
 }

 return (
 <Dialog open={open} onOpenChange={onOpenChange}>
 <DialogContent className="max-w-lg">
 <DialogHeader>
 <DialogTitle>{taskId ? 'Edit Task' : 'Create New Task'}</DialogTitle>
 </DialogHeader>

 <div className="space-y-4 py-2">
 <div className="space-y-1">
 <Label htmlFor="task-title">Title <span className="text-destructive">*</span></Label>
 <Input id="task-title" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Enter task title" />
 </div>

 <div className="space-y-1">
 <Label htmlFor="task-desc">Description <span className="text-destructive">*</span></Label>
 <Textarea id="task-desc" value={description} onChange={(e) => setDescription(e.target.value)} placeholder="Enter task description" rows={3} />
 </div>

 <div className="space-y-1">
 <Label htmlFor="task-due">Due Date <span className="text-destructive">*</span></Label>
 <Input id="task-due" type="date" value={dueDate} onChange={(e) => setDueDate(e.target.value)} />
 </div>

 <div className="space-y-1">
 <Label>Priority <span className="text-destructive">*</span></Label>
 <Select value={priority} onValueChange={setPriority}>
 <SelectTrigger>
 <SelectValue placeholder="Select priority" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="Low">Low</SelectItem>
 <SelectItem value="Medium">Medium</SelectItem>
 <SelectItem value="High">High</SelectItem>
 </SelectContent>
 </Select>
 </div>

 <div className="space-y-1">
 <Label>Goal</Label>
 <Select value={goalId || 'none'} onValueChange={(v) => setGoalId(v === 'none' ? '' : v)}>
 <SelectTrigger>
 <SelectValue placeholder="No goal (standalone task)" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="none">No goal (standalone task)</SelectItem>
 {goals.map((g) => (
 <SelectItem key={g.id} value={g.id}>
 {g.title}
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>

 <div className="space-y-1">
 <Label>Assign To</Label>
 <Select value={assignedUser || 'self'} onValueChange={(v) => setAssignedUser(v === 'self' ? '' : v)}>
 <SelectTrigger>
 <SelectValue placeholder="Select user (defaults to you)" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="self">Myself</SelectItem>
 {users.map((u) => (
 <SelectItem key={u.id} value={u.id}>
 {u.first_name} {u.last_name} ({u.username})
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>

 <div className="flex justify-end gap-2 pt-4">
 <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isSubmitting}>
 Cancel
 </Button>
 <Button onClick={handleSubmit} disabled={isSubmitting}>
 {isSubmitting ? (taskId ? 'Updating...' : 'Creating...') : (taskId ? 'Update Task' : 'Create Task')}
 </Button>
 </div>
 </div>
 </DialogContent>
 </Dialog>
 )
}
