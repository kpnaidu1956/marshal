import { useState, useEffect, useCallback } from 'react'
import { useParams, Link } from 'react-router-dom'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { StatusBadge, PriorityBadge } from '@/components/ui/StatusBadge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Textarea } from '@/components/ui/textarea'
import { Separator } from '@/components/ui/separator'
import { TaskDialog } from '@/components/TaskDialog'
import { toast } from 'sonner'
import { ArrowLeft, Loader2, Send, MessageSquare, Calendar, User, Hash, Pencil, Target, Lock, Globe } from 'lucide-react'
import type { Task, TaskComment } from '@/models/task'

interface OrgUser {
 id: string
 first_name: string
 last_name: string
}

function useOrgUsers(postgrestUrl: string, apiKey: string, orgId: string | undefined, token: string | null) {
 const [users, setUsers] = useState<OrgUser[]>([])
 const [userMap, setUserMap] = useState<Record<string, string>>({})

 useEffect(() => {
 if (!orgId) return
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().select('id,first_name,last_name').eq('organization_id', orgId).limit(200).build()
 client.get<OrgUser>('users', qs, token)
  .then((data) => {
  setUsers(data)
  const map: Record<string, string> = {}
  for (const u of data) map[u.id] = `${u.first_name} ${u.last_name}`
  setUserMap(map)
  })
  .catch(() => {})
 }, [orgId, token, postgrestUrl, apiKey])

 return { users, userMap }
}

export function TaskDetailPage() {
 const { id } = useParams<{ id: string }>()
 const token = useAuthStore((s) => s.token)
 const user = useAuthStore((s) => s.user)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [task, setTask] = useState<Task | null>(null)
 const [comments, setComments] = useState<TaskComment[]>([])
 const [assigneeName, setAssigneeName] = useState<string | null>(null)
 const [goalInfo, setGoalInfo] = useState<{ id: string; title: string } | null>(null)
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [commentText, setCommentText] = useState('')
 const [isPrivate, setIsPrivate] = useState(false)
 const [submittingComment, setSubmittingComment] = useState(false)
 const [editDialogOpen, setEditDialogOpen] = useState(false)
 const [mentionOpen, setMentionOpen] = useState(false)
 const [mentionFilter, setMentionFilter] = useState('')
 const [mentionIndex, setMentionIndex] = useState(0)

 const resolvedOrgId = currentOrg?.id || task?.organization_id
 const { users: orgUsers, userMap } = useOrgUsers(postgrestUrl, apiKey, resolvedOrgId, token)

 const loadTask = useCallback(async () => {
 if (!id) return
 setLoading(true)
 setError(null)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().select('*').eq('id', id).build()
 const data = await client.getOne<Task>('tasks', qs, token)
 setTask(data)
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to load task'
 setError(msg)
 } finally {
 setLoading(false)
 }
 }, [id, token, postgrestUrl, apiKey])

 const loadComments = useCallback(async () => {
 if (!id) return
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder()
 .eq('task_id', id)
 .order('created_at', true)
 .build()
 const data = await client.get<TaskComment>('task_comments', qs, token)
 setComments(data)
 } catch (err) {
 console.error('Failed to load comments:', err)
 }
 }, [id, token, postgrestUrl, apiKey])

 const resolveAssignee = useCallback(async (assignedTo: string | null) => {
 if (!assignedTo) {
 setAssigneeName(null)
 return
 }
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder()
 .select('id,first_name,last_name')
 .eq('id', assignedTo)
 .build()
 const data = await client.getOne<OrgUser>('users', qs, token)
 const name = [data.first_name, data.last_name].filter(Boolean).join(' ')
 setAssigneeName(name || assignedTo)
 } catch {
 setAssigneeName(assignedTo)
 }
 }, [token, postgrestUrl, apiKey])

 useEffect(() => {
 if (!id) return
 let cancelled = false

 const run = async () => {
 await loadTask()
 if (!cancelled) await loadComments()
 }
 run()

 return () => { cancelled = true }
 }, [id, loadTask, loadComments])

 useEffect(() => {
 if (task) {
 resolveAssignee(task.assigned_to)
 // Resolve goal name
 if (task.goal_id) {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().select('id,title').eq('id', task.goal_id).build()
 client.getOne<{ id: string; title: string }>('goals', qs, token)
 .then((g) => setGoalInfo(g))
 .catch(() => setGoalInfo(null))
 } else {
 setGoalInfo(null)
 }
 }
 }, [task?.assigned_to, task?.goal_id, resolveAssignee, postgrestUrl, apiKey, token])

 const handlePostComment = async () => {
 if (!commentText.trim() || !id || !user?.id) return
 setSubmittingComment(true)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 await client.post('task_comments', {
 task_id: id,
 organization_id: currentOrg?.id ?? task?.organization_id,
 author_id: user.id,
 content: commentText.trim(),
 is_private: isPrivate,
 }, token)
 setCommentText('')
 setIsPrivate(false)
 toast.success('Comment added')
 loadComments()
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to post comment'
 toast.error(msg)
 } finally {
 setSubmittingComment(false)
 }
 }

 const handleTaskUpdated = () => {
 loadTask()
 }

 if (loading) {
 return (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
 <span className="ml-2 text-sm text-muted-foreground">Loading task...</span>
 </div>
 )
 }

 if (error) {
 return (
 <div className="space-y-4">
 <Button variant="ghost" size="sm" asChild>
 <Link to="/tasks">
 <ArrowLeft className="mr-1 h-4 w-4" /> Back to tasks
 </Link>
 </Button>
 <Card>
 <CardContent className="py-6">
 <p className="text-sm text-destructive">{error}</p>
 </CardContent>
 </Card>
 </div>
 )
 }

 if (!task) {
 return (
 <div className="space-y-4">
 <Button variant="ghost" size="sm" asChild>
 <Link to="/tasks">
 <ArrowLeft className="mr-1 h-4 w-4" /> Back to tasks
 </Link>
 </Button>
 <Card>
 <CardContent className="py-6">
 <p className="text-sm text-muted-foreground">Task not found.</p>
 </CardContent>
 </Card>
 </div>
 )
 }

 return (
 <div className="space-y-6">
 {/* Top bar */}
 <div className="flex items-center justify-between">
 <Button variant="ghost" size="sm" asChild>
 <Link to="/tasks">
 <ArrowLeft className="mr-1 h-4 w-4" /> Back to tasks
 </Link>
 </Button>
 <Button variant="outline" size="sm" onClick={() => setEditDialogOpen(true)}>
 <Pencil className="mr-1 h-4 w-4" /> Edit
 </Button>
 </div>

 {/* Title section */}
 <div className="flex items-start justify-between">
 <div>
 <h1 className="text-2xl font-bold text-foreground">{task.title}</h1>
 {task.description && (
 <p className="mt-1 text-muted-foreground">{task.description}</p>
 )}
 </div>
 <div className="flex items-center gap-2">
 <StatusBadge status={task.status} />
 <PriorityBadge priority={task.priority} />
 </div>
 </div>

 {/* Info cards */}
 <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
 <Card>
 <CardContent className="p-4">
 <div className="flex items-center gap-1.5 text-xs text-muted-foreground uppercase mb-1">
 <Calendar className="h-3 w-3" />
 Due Date
 </div>
 <span className="block text-sm font-medium text-foreground">
 {task.due_date ?? 'No due date'}
 </span>
 </CardContent>
 </Card>

 <Card>
 <CardContent className="p-4">
 <div className="flex items-center gap-1.5 text-xs text-muted-foreground uppercase mb-1">
 <User className="h-3 w-3" />
 Assigned To
 </div>
 <span className="block text-sm font-medium text-foreground">
 {assigneeName ?? 'Unassigned'}
 </span>
 </CardContent>
 </Card>

 {task.task_number && (
 <Card>
 <CardContent className="p-4">
 <div className="flex items-center gap-1.5 text-xs text-muted-foreground uppercase mb-1">
 <Hash className="h-3 w-3" />
 Task #
 </div>
 <span className="block text-sm font-medium text-foreground">
 {task.task_number}
 </span>
 </CardContent>
 </Card>
 )}

 {goalInfo && (
 <Card>
 <CardContent className="p-4">
 <div className="flex items-center gap-1.5 text-xs text-muted-foreground uppercase mb-1">
 <Target className="h-3 w-3" />
 Goal
 </div>
 <Link to={`/goals/${goalInfo.id}`} className="block text-sm font-medium text-primary hover:underline">
 {goalInfo.title}
 </Link>
 </CardContent>
 </Card>
 )}
 </div>

 {/* Comments section */}
 <Card>
 <CardHeader>
 <CardTitle className="flex items-center gap-2 text-lg">
 <MessageSquare className="h-5 w-5" />
 Comments
 {comments.length > 0 && (
 <Badge variant="secondary" className="ml-1">{comments.length}</Badge>
 )}
 </CardTitle>
 </CardHeader>
 <CardContent className="space-y-4">
 {comments.length === 0 ? (
 <p className="text-sm text-muted-foreground">No comments yet.</p>
 ) : (
 comments.map((c) => (
 <div key={c.id} className={`rounded-lg border p-3 ${c.is_private ? 'bg-amber-50 border-amber-200' : ''}`}>
 <div className="flex items-center justify-between mb-1">
 <div className="flex items-center gap-2">
  <span className="text-xs font-medium text-foreground">{userMap[c.author_id] || 'Unknown'}</span>
  {c.is_private && (
  <span className="inline-flex items-center gap-0.5 text-[10px] text-amber-700 bg-amber-100 px-1.5 py-0.5 rounded-full">
   <Lock className="h-2.5 w-2.5" /> Private
  </span>
  )}
 </div>
 <span className="text-xs text-muted-foreground">
  {c.created_at ? new Date(c.created_at).toLocaleString() : ''}
 </span>
 </div>
 <p className="text-sm text-foreground">{c.content}</p>
 </div>
 ))
 )}

 <Separator />

 {/* New comment input */}
 <div className="space-y-2">
 <div className="relative">
  <Textarea
  placeholder="Write a comment... (type @ to mention someone)"
  value={commentText}
  onChange={(e) => {
   const val = e.target.value
   setCommentText(val)
   // Detect @ trigger
   const cursor = e.target.selectionStart ?? val.length
   const before = val.slice(0, cursor)
   const atMatch = before.match(/@(\w*)$/)
   if (atMatch) {
   setMentionOpen(true)
   setMentionFilter(atMatch[1].toLowerCase())
   setMentionIndex(0)
   } else {
   setMentionOpen(false)
   }
  }}
  onKeyDown={(e) => {
   if (!mentionOpen) return
   const filtered = orgUsers.filter((u) =>
   `${u.first_name} ${u.last_name}`.toLowerCase().includes(mentionFilter)
   )
   if (e.key === 'ArrowDown') {
   e.preventDefault()
   setMentionIndex((i) => Math.min(i + 1, filtered.length - 1))
   } else if (e.key === 'ArrowUp') {
   e.preventDefault()
   setMentionIndex((i) => Math.max(i - 1, 0))
   } else if (e.key === 'Enter' && filtered.length > 0) {
   e.preventDefault()
   const selected = filtered[mentionIndex]
   const name = `${selected.first_name} ${selected.last_name}`
   const cursor = (e.target as HTMLTextAreaElement).selectionStart ?? commentText.length
   const before = commentText.slice(0, cursor).replace(/@\w*$/, `@${name} `)
   const after = commentText.slice(cursor)
   setCommentText(before + after)
   setMentionOpen(false)
   } else if (e.key === 'Escape') {
   setMentionOpen(false)
   }
  }}
  rows={3}
  />
  {mentionOpen && (() => {
  const filtered = orgUsers.filter((u) =>
   `${u.first_name} ${u.last_name}`.toLowerCase().includes(mentionFilter)
  ).slice(0, 8)
  if (filtered.length === 0) return null
  return (
   <div className="absolute bottom-full mb-1 left-0 w-64 bg-white border border-gray-200 rounded-lg shadow-lg py-1 z-50 max-h-48 overflow-y-auto">
   {filtered.map((u, i) => (
    <button
    key={u.id}
    className={`block w-full text-left px-3 py-1.5 text-sm ${i === mentionIndex ? 'bg-primary/10 text-primary' : 'text-gray-700 hover:bg-gray-100'}`}
    onMouseDown={(e) => {
     e.preventDefault()
     const name = `${u.first_name} ${u.last_name}`
     setCommentText((prev) => prev.replace(/@\w*$/, `@${name} `))
     setMentionOpen(false)
    }}
    >
    {u.first_name} {u.last_name}
    </button>
   ))}
   </div>
  )
  })()}
 </div>
 <div className="flex items-center justify-between">
  <button
  type="button"
  onClick={() => setIsPrivate(!isPrivate)}
  className={`inline-flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded-md border transition-colors ${
   isPrivate
   ? 'bg-amber-50 border-amber-300 text-amber-700'
   : 'border-gray-200 text-gray-500 hover:bg-gray-50'
  }`}
  >
  {isPrivate ? <Lock className="h-3 w-3" /> : <Globe className="h-3 w-3" />}
  {isPrivate ? 'Private' : 'Public'}
  </button>
  <Button
  size="sm"
  onClick={handlePostComment}
  disabled={submittingComment || !commentText.trim()}
  >
  {submittingComment ? (
   <Loader2 className="mr-1 h-4 w-4 animate-spin" />
  ) : (
   <Send className="mr-1 h-4 w-4" />
  )}
  Send
  </Button>
 </div>
 </div>
 </CardContent>
 </Card>

 {/* Edit dialog */}
 <TaskDialog
 open={editDialogOpen}
 onOpenChange={setEditDialogOpen}
 onTaskCreated={handleTaskUpdated}
 taskId={task.id}
 />
 </div>
 )
}
