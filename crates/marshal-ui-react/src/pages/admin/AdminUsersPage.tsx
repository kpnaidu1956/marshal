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
 AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
 Select,
 SelectContent,
 SelectItem,
 SelectTrigger,
 SelectValue,
} from '@/components/ui/select'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import { Loader2, Plus, Pencil, Trash2, Users } from 'lucide-react'
import type { User } from '@/models/user'

const LEVEL_OPTIONS = ['Entry', 'Mid', 'Senior', 'Lead', 'Director'] as const

type UserFormData = {
 first_name: string
 last_name: string
 email: string
 username: string
 title: string
 level: string
 mobile_phone: string
}

const emptyForm: UserFormData = {
 first_name: '',
 last_name: '',
 email: '',
 username: '',
 title: '',
 level: '',
 mobile_phone: '',
}

export function AdminUsersPage() {
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [users, setUsers] = useState<User[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Dialog state
 const [dialogOpen, setDialogOpen] = useState(false)
 const [editingUser, setEditingUser] = useState<User | null>(null)
 const [form, setForm] = useState<UserFormData>(emptyForm)
 const [saving, setSaving] = useState(false)

 const orgId = currentOrg?.id

 const fetchUsers = useCallback(async () => {
 if (!orgId) return
 setLoading(true)
 setError(null)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder()
 .select(
 'id,organization_id,first_name,last_name,username,email,mobile_phone,title,level,is_deleted,created_at',
 )
 .eq('organization_id', orgId)
 .order('created_at', false)
 .limit(200)
 .build()
 const data = await client.get<User>('users', qs, token)
 setUsers(data)
 } catch (err: unknown) {
 const msg = err instanceof Error ? err.message : 'Failed to load users'
 setError(msg)
 } finally {
 setLoading(false)
 }
 }, [orgId, postgrestUrl, apiKey, token])

 useEffect(() => {
 fetchUsers()
 }, [fetchUsers])

 const openCreateDialog = () => {
 setEditingUser(null)
 setForm(emptyForm)
 setDialogOpen(true)
 }

 const openEditDialog = (user: User) => {
 setEditingUser(user)
 setForm({
 first_name: user.first_name ?? '',
 last_name: user.last_name ?? '',
 email: user.email ?? '',
 username: user.username ?? '',
 title: user.title ?? '',
 level: user.level ?? '',
 mobile_phone: user.mobile_phone ?? '',
 })
 setDialogOpen(true)
 }

 const handleSave = async () => {
 if (!form.first_name.trim() && !form.last_name.trim()) {
 toast.error('At least first name or last name is required')
 return
 }

 setSaving(true)
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const payload: Record<string, unknown> = {
 first_name: form.first_name.trim() || null,
 last_name: form.last_name.trim() || null,
 email: form.email.trim() || null,
 username: form.username.trim() || null,
 title: form.title.trim() || null,
 level: form.level || null,
 mobile_phone: form.mobile_phone.trim() || null,
 }

 if (editingUser) {
 const qs = new QueryBuilder().eq('id', editingUser.id).build()
 await client.patch('users', qs, payload, token)
 toast.success('User updated successfully')
 } else {
 payload.organization_id = orgId
 await client.post('users', payload, token)
 toast.success('User created successfully')
 }

 setDialogOpen(false)
 fetchUsers()
 } catch (err: unknown) {
 const msg = err instanceof Error ? err.message : 'Failed to save user'
 console.error('Error saving user:', err)
 toast.error(msg)
 } finally {
 setSaving(false)
 }
 }

 const handleDelete = async (user: User) => {
 try {
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().eq('id', user.id).build()
 await client.delete('users', qs, token)
 toast.success('User deleted successfully')
 fetchUsers()
 } catch (err: unknown) {
 const msg = err instanceof Error ? err.message : 'Failed to delete user'
 console.error('Error deleting user:', err)
 toast.error(msg)
 }
 }

 const updateField = (field: keyof UserFormData, value: string) => {
 setForm((prev) => ({ ...prev, [field]: value }))
 }

 return (
 <div className="space-y-4">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-foreground">Users</h1>
 <Button onClick={openCreateDialog} size="sm">
 <Plus className="mr-2 h-4 w-4" />
 Add User
 </Button>
 </div>

 {/* Stats */}
 <div className="flex items-center gap-2">
 <Users className="h-4 w-4 text-muted-foreground" />
 <span className="text-sm text-muted-foreground">Total Users</span>
 <Badge variant="secondary">{users.length}</Badge>
 </div>

 {error && (
 <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
 {error}
 </div>
 )}

 {loading && (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
 <span className="ml-2 text-sm text-muted-foreground">Loading users...</span>
 </div>
 )}

 {!loading && (
 <Card>
 <CardHeader className="pb-3">
 <CardTitle className="text-base">User Directory</CardTitle>
 </CardHeader>
 <CardContent className="p-0">
 <div className="overflow-x-auto">
 <table className="w-full text-left min-w-[800px]">
 <thead>
 <tr className="border-b bg-muted/50">
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Name
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Email
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Username
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Title
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Level
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase text-right">
 Actions
 </th>
 </tr>
 </thead>
 <tbody>
 {users.map((u) => (
 <tr
 key={u.id}
 className="border-b last:border-0 hover:bg-muted/30 transition-colors"
 >
 <td className="px-4 py-3 text-sm font-medium text-foreground">
 {[u.first_name, u.last_name].filter(Boolean).join(' ') || '--'}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {u.email ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {u.username ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {u.title ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {u.level ?? '--'}
 </td>
 <td className="px-4 py-3 text-right">
 <div className="flex items-center justify-end gap-1">
 <Button
 variant="ghost"
 size="icon"
 className="h-8 w-8"
 onClick={() => openEditDialog(u)}
 >
 <Pencil className="h-4 w-4" />
 </Button>
 <AlertDialog>
 <AlertDialogTrigger asChild>
 <Button
 variant="ghost"
 size="icon"
 className="h-8 w-8 text-destructive hover:text-destructive"
 >
 <Trash2 className="h-4 w-4" />
 </Button>
 </AlertDialogTrigger>
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>Delete User</AlertDialogTitle>
 <AlertDialogDescription>
 Are you sure you want to delete{' '}
 <span className="font-medium">
 {[u.first_name, u.last_name].filter(Boolean).join(' ') ||
 u.email ||
 'this user'}
 </span>
 ? This action cannot be undone.
 </AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel>Cancel</AlertDialogCancel>
 <AlertDialogAction
 onClick={() => handleDelete(u)}
 className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
 >
 Delete
 </AlertDialogAction>
 </AlertDialogFooter>
 </AlertDialogContent>
 </AlertDialog>
 </div>
 </td>
 </tr>
 ))}
 {users.length === 0 && (
 <tr>
 <td
 colSpan={6}
 className="px-4 py-8 text-center text-sm text-muted-foreground"
 >
 No users found.
 </td>
 </tr>
 )}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}

 {/* Create / Edit Dialog */}
 <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
 <DialogContent className="max-w-md">
 <DialogHeader>
 <DialogTitle>{editingUser ? 'Edit User' : 'Create User'}</DialogTitle>
 </DialogHeader>

 <div className="space-y-4">
 <div className="grid grid-cols-2 gap-3">
 <div>
 <Label htmlFor="user-first-name">First Name</Label>
 <Input
 id="user-first-name"
 value={form.first_name}
 onChange={(e) => updateField('first_name', e.target.value)}
 placeholder="First name"
 />
 </div>
 <div>
 <Label htmlFor="user-last-name">Last Name</Label>
 <Input
 id="user-last-name"
 value={form.last_name}
 onChange={(e) => updateField('last_name', e.target.value)}
 placeholder="Last name"
 />
 </div>
 </div>

 <div>
 <Label htmlFor="user-email">Email</Label>
 <Input
 id="user-email"
 type="email"
 value={form.email}
 onChange={(e) => updateField('email', e.target.value)}
 placeholder="user@example.com"
 />
 </div>

 <div>
 <Label htmlFor="user-username">Username</Label>
 <Input
 id="user-username"
 value={form.username}
 onChange={(e) => updateField('username', e.target.value)}
 placeholder="Username"
 />
 </div>

 <div>
 <Label htmlFor="user-title">Title</Label>
 <Input
 id="user-title"
 value={form.title}
 onChange={(e) => updateField('title', e.target.value)}
 placeholder="Job title"
 />
 </div>

 <div>
 <Label htmlFor="user-level">Level</Label>
 <Select
 value={form.level || 'none'}
 onValueChange={(v) => updateField('level', v === 'none' ? '' : v)}
 >
 <SelectTrigger>
 <SelectValue placeholder="Select level" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="none">No level</SelectItem>
 {LEVEL_OPTIONS.map((lvl) => (
 <SelectItem key={lvl} value={lvl}>
 {lvl}
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>

 <div>
 <Label htmlFor="user-mobile">Mobile Phone</Label>
 <Input
 id="user-mobile"
 type="tel"
 value={form.mobile_phone}
 onChange={(e) => updateField('mobile_phone', e.target.value)}
 placeholder="+1234567890"
 />
 </div>
 </div>

 <DialogFooter>
 <Button variant="outline" onClick={() => setDialogOpen(false)}>
 Cancel
 </Button>
 <Button onClick={handleSave} disabled={saving}>
 {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
 {editingUser ? 'Update' : 'Create'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
