import { useState, useEffect, useCallback } from 'react'
import {
 Dialog,
 DialogContent,
 DialogHeader,
 DialogTitle,
 DialogFooter,
} from '@/components/ui/dialog'
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
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import {
 Select,
 SelectContent,
 SelectItem,
 SelectTrigger,
 SelectValue,
} from '@/components/ui/select'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Separator } from '@/components/ui/separator'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import {
 Loader2,
 Plus,
 Mail,
 MailOpen,
 Archive,
 Trash2,
 Send,
 Inbox,
} from 'lucide-react'
import type { Message } from '@/models/message'
import type { User } from '@/models/user'

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatMessageDate(iso: string | null): string {
 if (!iso) return ''
 const date = new Date(iso)
 const now = new Date()
 const isToday =
 date.getFullYear() === now.getFullYear() &&
 date.getMonth() === now.getMonth() &&
 date.getDate() === now.getDate()
 if (isToday) {
 return date.toLocaleTimeString(undefined, {
 hour: 'numeric',
 minute: '2-digit',
 })
 }
 return date.toLocaleDateString(undefined, {
 month: 'short',
 day: 'numeric',
 year:
 date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
 })
}

function userName(user: User | undefined): string {
 if (!user) return 'Unknown'
 const parts = [user.first_name, user.last_name].filter(Boolean)
 return parts.length > 0 ? parts.join(' ') : user.email ?? 'Unknown'
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function MessagesPage() {
 const token = useAuthStore((s) => s.token)
 const currentUser = useAuthStore((s) => s.user)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')

 const [messages, setMessages] = useState<Message[]>([])
 const [users, setUsers] = useState<User[]>([])
 const [loading, setLoading] = useState(true)
 const [selectedId, setSelectedId] = useState<string | null>(null)
 const [showArchived, setShowArchived] = useState(false)
 const [composeOpen, setComposeOpen] = useState(false)
 const [deleteTarget, setDeleteTarget] = useState<string | null>(null)
 const [sending, setSending] = useState(false)
 const [emailPickerOpen, setEmailPickerOpen] = useState(false)

 // Compose form state
 const [composeRecipient, setComposeRecipient] = useState('')
 const [composeSubject, setComposeSubject] = useState('')
 const [composeContent, setComposeContent] = useState('')

 const selected = messages.find((m) => m.id === selectedId) ?? null
 const unreadCount = messages.filter(
 (m) => !m.is_read && !m.is_archived,
 ).length

 const usersMap = new Map(users.map((u) => [u.id, u]))

 // Build a client on each call — cheap, no state needed
 const getClient = useCallback(() => {
 const { postgrestUrl, apiKey } = detectApiUrls()
 return new PostgRestClient(postgrestUrl, apiKey)
 }, [])

 // ------ Fetch messages ------
 const fetchMessages = useCallback(async () => {
 if (!orgId) return
 setLoading(true)
 try {
 const client = getClient()
 const query = new QueryBuilder()
 .select(
 'id,organization_id,sender_id,recipient_id,subject,content,is_read,is_archived,created_at',
 )
 .eq('organization_id', orgId)
 .eq('is_archived', showArchived ? 'true' : 'false')
 .order('created_at', false)
 .limit(50)
 .build()
 const data = await client.get<Message>('messages', query, token)
 setMessages(data)
 } catch (err) {
 toast.error('Failed to load messages')
 console.error(err)
 } finally {
 setLoading(false)
 }
 }, [orgId, token, showArchived, getClient])

 // ------ Fetch users ------
 const fetchUsers = useCallback(async () => {
 if (!orgId) return
 try {
 const client = getClient()
 const query = new QueryBuilder()
 .select('id,first_name,last_name,email')
 .eq('organization_id', orgId)
 .order('first_name', true)
 .build()
 const data = await client.get<User>('users', query, token)
 setUsers(data)
 } catch (err) {
 console.error('Failed to load users', err)
 }
 }, [orgId, token, getClient])

 useEffect(() => {
 fetchMessages()
 fetchUsers()
 }, [fetchMessages, fetchUsers])

 // ------ Mark as read ------
 const markAsRead = useCallback(
 async (msg: Message) => {
 if (msg.is_read) return
 try {
 const client = getClient()
 const query = new QueryBuilder().eq('id', msg.id).build()
 await client.patch<Message>(
 'messages',
 query,
 { is_read: true },
 token,
 )
 setMessages((prev) =>
 prev.map((m) =>
 m.id === msg.id ? { ...m, is_read: true } : m,
 ),
 )
 } catch (err) {
 console.error('Failed to mark as read', err)
 }
 },
 [token, getClient],
 )

 // ------ Select message ------
 const handleSelect = useCallback(
 (msg: Message) => {
 setSelectedId(msg.id)
 markAsRead(msg)
 },
 [markAsRead],
 )

 // ------ Archive ------
 const handleArchive = useCallback(
 async (msgId: string) => {
 try {
 const client = getClient()
 const query = new QueryBuilder().eq('id', msgId).build()
 await client.patch<Message>(
 'messages',
 query,
 { is_archived: true },
 token,
 )
 setMessages((prev) => prev.filter((m) => m.id !== msgId))
 if (selectedId === msgId) setSelectedId(null)
 toast.success('Message archived')
 } catch (err) {
 toast.error('Failed to archive message')
 console.error(err)
 }
 },
 [token, selectedId, getClient],
 )

 // ------ Delete ------
 const handleDelete = useCallback(
 async (msgId: string) => {
 try {
 const client = getClient()
 const query = new QueryBuilder().eq('id', msgId).build()
 await client.delete('messages', query, token)
 setMessages((prev) => prev.filter((m) => m.id !== msgId))
 if (selectedId === msgId) setSelectedId(null)
 setDeleteTarget(null)
 toast.success('Message deleted')
 } catch (err) {
 toast.error('Failed to delete message')
 console.error(err)
 }
 },
 [token, selectedId, getClient],
 )

 // ------ Send new message ------
 const handleSend = useCallback(async () => {
 if (!orgId || !currentUser) return
 if (!composeRecipient || !composeContent.trim()) {
 toast.error('Recipient and content are required')
 return
 }
 setSending(true)
 try {
 const client = getClient()
 await client.post<Message>(
 'messages',
 {
 organization_id: orgId,
 sender_id: currentUser.id,
 recipient_id: composeRecipient,
 subject: composeSubject || null,
 content: composeContent,
 is_read: false,
 is_archived: false,
 },
 token,
 )
 toast.success('Message sent')
 setComposeOpen(false)
 setComposeRecipient('')
 setComposeSubject('')
 setComposeContent('')
 fetchMessages()
 } catch (err) {
 toast.error('Failed to send message')
 console.error(err)
 } finally {
 setSending(false)
 }
 }, [
 orgId,
 currentUser,
 composeRecipient,
 composeSubject,
 composeContent,
 token,
 getClient,
 fetchMessages,
 ])

 // ------ Email popup ------
 const handleEmailClick = useCallback(() => {
 const saved = localStorage.getItem('marshal_email_provider')
 if (saved === 'gmail' || saved === 'outlook') {
  const url = saved === 'gmail'
  ? 'https://mail.google.com'
  : 'https://outlook.live.com/mail/'
  window.open(url, 'emailPopup', 'width=1200,height=800,menubar=no,toolbar=no,location=no,status=no')
 } else {
  setEmailPickerOpen(true)
 }
 }, [])

 const handleEmailProviderSelect = useCallback((provider: 'gmail' | 'outlook') => {
 localStorage.setItem('marshal_email_provider', provider)
 setEmailPickerOpen(false)
 const url = provider === 'gmail'
  ? 'https://mail.google.com'
  : 'https://outlook.live.com/mail/'
 window.open(url, 'emailPopup', 'width=1200,height=800,menubar=no,toolbar=no,location=no,status=no')
 }, [])

 // ------------------------------------------------------------------
 // Render
 // ------------------------------------------------------------------

 return (
 <div className="space-y-4">
 {/* Header */}
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-foreground">Messages</h1>
 <div className="flex items-center gap-2">
  <Button onClick={handleEmailClick} size="sm" variant="outline">
  <Mail className="mr-1.5 h-4 w-4" />
  Email
  </Button>
  <Button onClick={() => setComposeOpen(true)} size="sm">
  <Plus className="mr-1.5 h-4 w-4" />
  Compose
  </Button>
 </div>
 </div>

 {/* Filter tabs */}
 <div className="flex items-center gap-2">
 <Button
 variant={!showArchived ? 'default' : 'outline'}
 size="sm"
 onClick={() => {
 setShowArchived(false)
 setSelectedId(null)
 }}
 >
 <Inbox className="mr-1.5 h-4 w-4" />
 Inbox
 {unreadCount > 0 && !showArchived && (
 <Badge variant="secondary" className="ml-1.5">
 {unreadCount}
 </Badge>
 )}
 </Button>
 <Button
 variant={showArchived ? 'default' : 'outline'}
 size="sm"
 onClick={() => {
 setShowArchived(true)
 setSelectedId(null)
 }}
 >
 <Archive className="mr-1.5 h-4 w-4" />
 Archived
 </Button>
 </div>

 {/* Loading */}
 {loading && (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
 </div>
 )}

 {/* Main two-panel layout */}
 {!loading && (
 <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
 {/* Left panel — message list */}
 <Card className="md:col-span-1">
 <ScrollArea className="h-[calc(100vh-16rem)]">
 {messages.length === 0 ? (
 <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
 <Mail className="h-10 w-10 mb-3" />
 <p className="text-sm">
 {showArchived
 ? 'No archived messages'
 : 'No messages in your inbox'}
 </p>
 </div>
 ) : (
 <div className="divide-y divide-border">
 {messages.map((m) => {
 const isRead = m.is_read ?? false
 const isSelected = m.id === selectedId
 const subject = m.subject || '(no subject)'
 const preview =
 m.content.length > 80
 ? m.content.slice(0, 80) + '...'
 : m.content

 return (
 <button
 key={m.id}
 type="button"
 onClick={() => handleSelect(m)}
 className={`w-full text-left px-4 py-3 transition-colors hover:bg-muted/50 ${
 isSelected ? 'bg-muted' : ''
 }`}
 >
 <div className="flex items-start gap-2">
 {!isRead && (
 <span className="mt-1.5 h-2 w-2 shrink-0 rounded-full bg-primary" />
 )}
 <div className="min-w-0 flex-1">
 <div className="flex items-center justify-between gap-2">
 <p
 className={`truncate text-sm ${
 !isRead
 ? 'font-semibold text-foreground'
 : 'text-foreground'
 }`}
 >
 {subject}
 </p>
 <span className="shrink-0 text-xs text-muted-foreground">
 {formatMessageDate(m.created_at)}
 </span>
 </div>
 <p className="mt-0.5 truncate text-xs text-muted-foreground">
 {userName(usersMap.get(m.sender_id ?? ''))}
 </p>
 <p className="mt-0.5 truncate text-xs text-muted-foreground/70">
 {preview}
 </p>
 </div>
 </div>
 </button>
 )
 })}
 </div>
 )}
 </ScrollArea>
 </Card>

 {/* Right panel — message detail */}
 <Card className="md:col-span-2">
 {selected ? (
 <CardContent className="p-6">
 {/* Subject + actions */}
 <div className="flex items-start justify-between gap-4">
 <div className="min-w-0">
 <h2 className="text-lg font-semibold text-foreground">
 {selected.subject || '(no subject)'}
 </h2>
 <p className="mt-1 text-sm text-muted-foreground">
 From{' '}
 <span className="font-medium text-foreground">
 {userName(
 usersMap.get(selected.sender_id ?? ''),
 )}
 </span>
 {selected.recipient_id && (
 <>
 {' '}to{' '}
 <span className="font-medium text-foreground">
 {userName(
 usersMap.get(selected.recipient_id),
 )}
 </span>
 </>
 )}
 </p>
 <p className="mt-0.5 text-xs text-muted-foreground">
 {selected.created_at
 ? new Date(selected.created_at).toLocaleString()
 : ''}
 </p>
 </div>
 <div className="flex items-center gap-1">
 {!showArchived && (
 <Button
 variant="outline"
 size="sm"
 onClick={() => handleArchive(selected.id)}
 >
 <Archive className="mr-1.5 h-4 w-4" />
 Archive
 </Button>
 )}
 <Button
 variant="outline"
 size="sm"
 onClick={() => setDeleteTarget(selected.id)}
 className="text-destructive hover:text-destructive"
 >
 <Trash2 className="mr-1.5 h-4 w-4" />
 Delete
 </Button>
 </div>
 </div>

 <Separator className="my-4" />

 {/* Body */}
 <div className="whitespace-pre-wrap text-sm text-foreground leading-relaxed">
 {selected.content}
 </div>

 {/* Read status indicator */}
 <div className="mt-6 flex items-center gap-1.5 text-xs text-muted-foreground">
 {selected.is_read ? (
 <>
 <MailOpen className="h-3.5 w-3.5" />
 Read
 </>
 ) : (
 <>
 <Mail className="h-3.5 w-3.5" />
 Unread
 </>
 )}
 </div>
 </CardContent>
 ) : (
 <div className="flex flex-col items-center justify-center h-[calc(100vh-16rem)] text-muted-foreground">
 <Mail className="h-10 w-10 mb-3" />
 <p className="text-sm">Select a message to read</p>
 </div>
 )}
 </Card>
 </div>
 )}

 {/* Compose dialog */}
 <Dialog open={composeOpen} onOpenChange={setComposeOpen}>
 <DialogContent className="sm:max-w-lg">
 <DialogHeader>
 <DialogTitle>New Message</DialogTitle>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="compose-recipient">Recipient</Label>
 <Select
 value={composeRecipient}
 onValueChange={setComposeRecipient}
 >
 <SelectTrigger id="compose-recipient">
 <SelectValue placeholder="Select recipient" />
 </SelectTrigger>
 <SelectContent>
 {users
 .filter((u) => u.id !== currentUser?.id)
 .map((u) => (
 <SelectItem key={u.id} value={u.id}>
 {userName(u)}
 {u.email ? ` (${u.email})` : ''}
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 <div className="space-y-2">
 <Label htmlFor="compose-subject">Subject</Label>
 <Input
 id="compose-subject"
 placeholder="Optional subject"
 value={composeSubject}
 onChange={(e) => setComposeSubject(e.target.value)}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="compose-content">Message</Label>
 <Textarea
 id="compose-content"
 placeholder="Write your message..."
 rows={5}
 value={composeContent}
 onChange={(e) => setComposeContent(e.target.value)}
 />
 </div>
 </div>
 <DialogFooter>
 <Button
 variant="outline"
 onClick={() => setComposeOpen(false)}
 disabled={sending}
 >
 Cancel
 </Button>
 <Button onClick={handleSend} disabled={sending}>
 {sending ? (
 <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
 ) : (
 <Send className="mr-1.5 h-4 w-4" />
 )}
 Send
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete confirmation */}
 <AlertDialog
 open={deleteTarget !== null}
 onOpenChange={(open) => {
 if (!open) setDeleteTarget(null)
 }}
 >
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>Delete message?</AlertDialogTitle>
 <AlertDialogDescription>
 This action cannot be undone. The message will be
 permanently removed.
 </AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel>Cancel</AlertDialogCancel>
 <AlertDialogAction
 onClick={() => deleteTarget && handleDelete(deleteTarget)}
 className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
 >
 Delete
 </AlertDialogAction>
 </AlertDialogFooter>
 </AlertDialogContent>
 </AlertDialog>

 {/* Email provider picker */}
 <Dialog open={emailPickerOpen} onOpenChange={setEmailPickerOpen}>
 <DialogContent className="sm:max-w-sm">
 <DialogHeader>
  <DialogTitle>Choose your email provider</DialogTitle>
 </DialogHeader>
 <div className="flex flex-col gap-3 py-4">
  <button
  onClick={() => handleEmailProviderSelect('gmail')}
  className="flex items-center gap-3 rounded-lg border border-gray-200 p-4 hover:bg-gray-50 transition-colors text-left"
  >
  <div className="h-10 w-10 rounded-lg bg-red-50 flex items-center justify-center text-red-600 font-bold text-lg">G</div>
  <div>
   <p className="text-sm font-semibold text-foreground">Gmail</p>
   <p className="text-xs text-muted-foreground">Open Google Mail</p>
  </div>
  </button>
  <button
  onClick={() => handleEmailProviderSelect('outlook')}
  className="flex items-center gap-3 rounded-lg border border-gray-200 p-4 hover:bg-gray-50 transition-colors text-left"
  >
  <div className="h-10 w-10 rounded-lg bg-blue-50 flex items-center justify-center text-blue-600 font-bold text-lg">O</div>
  <div>
   <p className="text-sm font-semibold text-foreground">Outlook</p>
   <p className="text-xs text-muted-foreground">Open Microsoft Outlook</p>
  </div>
  </button>
 </div>
 <p className="text-xs text-muted-foreground text-center">Your choice will be remembered for next time.</p>
 </DialogContent>
 </Dialog>
 </div>
 )
}
