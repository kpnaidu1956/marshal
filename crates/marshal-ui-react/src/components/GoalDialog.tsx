import { useState, useEffect, useMemo } from 'react'
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
import type { Goal } from '@/models/goal'

type GoalDialogProps = {
 open: boolean
 onOpenChange: (open: boolean) => void
 onGoalSaved: () => void
 editingGoal?: Goal | null
 parentGoalId?: string | null
 allGoals?: Goal[]
}

export function GoalDialog({
 open,
 onOpenChange,
 onGoalSaved,
 editingGoal,
 parentGoalId,
 allGoals = [],
}: GoalDialogProps) {
 const user = useAuthStore((s) => s.user)
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [title, setTitle] = useState('')
 const [description, setDescription] = useState('')
 const [status, setStatus] = useState('not_started')
 const [targetDate, setTargetDate] = useState('')
 const [selectedParentId, setSelectedParentId] = useState<string | null>(null)
 const [saving, setSaving] = useState(false)

 useEffect(() => {
 if (editingGoal) {
 setTitle(editingGoal.title)
 setDescription(editingGoal.description || '')
 setStatus(editingGoal.status || 'not_started')
 setTargetDate(editingGoal.target_date || '')
 setSelectedParentId(editingGoal.parent_goal_id)
 } else {
 setTitle('')
 setDescription('')
 setStatus('not_started')
 setTargetDate('')
 setSelectedParentId(parentGoalId || null)
 }
 }, [editingGoal, parentGoalId, open])

 const handleSave = async () => {
 if (!title.trim()) {
 toast.error('Title is required')
 return
 }
 if (!currentOrg?.id) {
 toast.error('No organization selected')
 return
 }

 setSaving(true)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const goalData = {
 title: title.trim(),
 description: description.trim() || null,
 status,
 target_date: targetDate || null,
 parent_goal_id: selectedParentId,
 created_by: user?.id || null,
 organization_id: currentOrg?.id || null,
 }

 if (editingGoal) {
 const qs = new QueryBuilder().eq('id', editingGoal.id).build()
 await client.patch('goals', qs, goalData, token)
 toast.success('Goal updated successfully')
 } else {
 await client.post('goals', goalData, token)
 toast.success('Goal created successfully')
 }

 onGoalSaved()
 onOpenChange(false)
 } catch (error: unknown) {
 const msg = error instanceof Error ? error.message : 'Failed to save goal'
 console.error('Error saving goal:', error)
 toast.error(msg)
 } finally {
 setSaving(false)
 }
 }

 // Filter out the current goal and its descendants from parent options
 const availableParents = useMemo(() => {
 if (!editingGoal) return allGoals

 const getDescendantIds = (goalId: string): string[] => {
 const children = allGoals.filter((g) => g.parent_goal_id === goalId)
 return [goalId, ...children.flatMap((c) => getDescendantIds(c.id))]
 }

 const excludeIds = getDescendantIds(editingGoal.id)
 return allGoals.filter((g) => !excludeIds.includes(g.id))
 }, [allGoals, editingGoal?.id])

 return (
 <Dialog open={open} onOpenChange={onOpenChange}>
 <DialogContent className="max-w-md">
 <DialogHeader>
 <DialogTitle>
 {editingGoal ? 'Edit Goal' : parentGoalId ? 'Create Sub-Goal' : 'Create Goal'}
 </DialogTitle>
 </DialogHeader>

 <div className="space-y-4">
 <div>
 <Label htmlFor="goal-title">Title *</Label>
 <Input
 id="goal-title"
 value={title}
 onChange={(e) => setTitle(e.target.value)}
 placeholder="Enter goal title"
 />
 </div>

 <div>
 <Label htmlFor="goal-description">Description</Label>
 <Textarea
 id="goal-description"
 value={description}
 onChange={(e) => setDescription(e.target.value)}
 placeholder="Enter goal description"
 rows={3}
 />
 </div>

 <div>
 <Label htmlFor="goal-status">Status</Label>
 <Select value={status} onValueChange={setStatus}>
 <SelectTrigger>
 <SelectValue />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="not_started">Not Started</SelectItem>
 <SelectItem value="in_progress">In Progress</SelectItem>
 <SelectItem value="completed">Completed</SelectItem>
 <SelectItem value="on_hold">On Hold</SelectItem>
 </SelectContent>
 </Select>
 </div>

 <div>
 <Label htmlFor="goal-target-date">Target Date</Label>
 <Input
 id="goal-target-date"
 type="date"
 value={targetDate}
 onChange={(e) => setTargetDate(e.target.value)}
 />
 </div>

 <div>
 <Label htmlFor="goal-parent">Parent Goal</Label>
 <Select
 value={selectedParentId || 'none'}
 onValueChange={(v) => setSelectedParentId(v === 'none' ? null : v)}
 >
 <SelectTrigger>
 <SelectValue placeholder="No parent (top-level goal)" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="none">No parent (top-level goal)</SelectItem>
 {availableParents.map((goal) => (
 <SelectItem key={goal.id} value={goal.id}>
 {goal.title}
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>

 <div className="flex justify-end gap-2 pt-4">
 <Button variant="outline" onClick={() => onOpenChange(false)}>
 Cancel
 </Button>
 <Button onClick={handleSave} disabled={saving}>
 {saving ? 'Saving...' : editingGoal ? 'Update' : 'Create'}
 </Button>
 </div>
 </div>
 </DialogContent>
 </Dialog>
 )
}
