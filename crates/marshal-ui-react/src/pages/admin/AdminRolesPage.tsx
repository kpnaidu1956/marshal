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
import { Textarea } from '@/components/ui/textarea'
import { Checkbox } from '@/components/ui/checkbox'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import { Loader2, Plus, Pencil, Trash2, Shield } from 'lucide-react'
import type { Role } from '@/models/role'

export function AdminRolesPage() {
 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')

 const [roles, setRoles] = useState<Role[]>([])
 const [loading, setLoading] = useState(true)
 const [dialogOpen, setDialogOpen] = useState(false)
 const [editingRole, setEditingRole] = useState<Role | null>(null)
 const [saving, setSaving] = useState(false)

 // Form state
 const [formName, setFormName] = useState('')
 const [formDescription, setFormDescription] = useState('')
 const [formIsSystem, setFormIsSystem] = useState(false)

 const buildClient = useCallback(() => {
 const { postgrestUrl, apiKey } = detectApiUrls()
 return new PostgRestClient(postgrestUrl, apiKey)
 }, [])

 const fetchRoles = useCallback(async () => {
 if (!orgId) return
 setLoading(true)
 try {
 const client = buildClient()
 const query = new QueryBuilder()
 .select('id,organization_id,name,description,is_system,created_at')
 .eq('organization_id', orgId)
 .order('name', true)
 .build()
 const data = await client.get<Role>('roles', query, token)
 setRoles(data)
 } catch (err) {
 toast.error('Failed to load roles')
 console.error(err)
 } finally {
 setLoading(false)
 }
 }, [orgId, token, buildClient])

 useEffect(() => {
 fetchRoles()
 }, [fetchRoles])

 function openCreateDialog() {
 setEditingRole(null)
 setFormName('')
 setFormDescription('')
 setFormIsSystem(false)
 setDialogOpen(true)
 }

 function openEditDialog(role: Role) {
 setEditingRole(role)
 setFormName(role.name)
 setFormDescription(role.description ?? '')
 setFormIsSystem(role.is_system ?? false)
 setDialogOpen(true)
 }

 async function handleSave() {
 if (!formName.trim()) {
 toast.error('Name is required')
 return
 }
 if (!orgId) return

 setSaving(true)
 try {
 const client = buildClient()

 if (editingRole) {
 await client.patch<Role>(
 'roles',
 new QueryBuilder().eq('id', editingRole.id).build(),
 { name: formName.trim(), description: formDescription.trim() || null },
 token,
 )
 toast.success('Role updated')
 } else {
 await client.post<Role>(
 'roles',
 {
 organization_id: orgId,
 name: formName.trim(),
 description: formDescription.trim() || null,
 is_system: formIsSystem,
 },
 token,
 )
 toast.success('Role created')
 }

 setDialogOpen(false)
 await fetchRoles()
 } catch (err) {
 toast.error(editingRole ? 'Failed to update role' : 'Failed to create role')
 console.error(err)
 } finally {
 setSaving(false)
 }
 }

 async function handleDelete(role: Role) {
 try {
 const client = buildClient()
 await client.delete(
 'roles',
 new QueryBuilder().eq('id', role.id).build(),
 token,
 )
 toast.success('Role deleted')
 await fetchRoles()
 } catch (err) {
 toast.error('Failed to delete role')
 console.error(err)
 }
 }

 return (
 <div className="space-y-4">
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
 <div className="flex items-center gap-3">
 <CardTitle className="text-2xl font-bold text-foreground flex items-center gap-2">
 <Shield className="h-6 w-6" />
 Roles
 </CardTitle>
 {!loading && (
 <Badge variant="secondary">{roles.length}</Badge>
 )}
 </div>
 <Button onClick={openCreateDialog} size="sm">
 <Plus className="h-4 w-4 mr-1" />
 Add Role
 </Button>
 </CardHeader>

 <CardContent>
 {loading ? (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
 </div>
 ) : (
 <div className="overflow-x-auto">
 <table className="w-full text-left min-w-[640px]">
 <thead>
 <tr className="border-b">
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Name
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Description
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Type
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Created
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase text-right">
 Actions
 </th>
 </tr>
 </thead>
 <tbody>
 {roles.map((r) => (
 <tr
 key={r.id}
 className="border-b last:border-0 hover:bg-muted/50"
 >
 <td className="px-4 py-3 text-sm font-medium text-foreground">
 {r.name}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {r.description ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm">
 {r.is_system ? (
 <Badge className="bg-green-100 text-green-800">
 System
 </Badge>
 ) : (
 <Badge variant="secondary">Custom</Badge>
 )}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {r.created_at?.slice(0, 10) ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm text-right">
 <div className="flex items-center justify-end gap-1">
 <Button
 variant="ghost"
 size="icon"
 onClick={() => openEditDialog(r)}
 title="Edit role"
 >
 <Pencil className="h-4 w-4" />
 </Button>

 {!r.is_system && (
 <AlertDialog>
 <AlertDialogTrigger asChild>
 <Button
 variant="ghost"
 size="icon"
 title="Delete role"
 >
 <Trash2 className="h-4 w-4 text-destructive" />
 </Button>
 </AlertDialogTrigger>
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>Delete role</AlertDialogTitle>
 <AlertDialogDescription>
 Are you sure you want to delete the role &quot;{r.name}&quot;?
 This action cannot be undone.
 </AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel>Cancel</AlertDialogCancel>
 <AlertDialogAction
 onClick={() => handleDelete(r)}
 className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
 >
 Delete
 </AlertDialogAction>
 </AlertDialogFooter>
 </AlertDialogContent>
 </AlertDialog>
 )}
 </div>
 </td>
 </tr>
 ))}
 {roles.length === 0 && (
 <tr>
 <td
 colSpan={5}
 className="px-4 py-8 text-center text-sm text-muted-foreground"
 >
 No roles found.
 </td>
 </tr>
 )}
 </tbody>
 </table>
 </div>
 )}
 </CardContent>
 </Card>

 {/* Create / Edit Dialog */}
 <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
 <DialogContent>
 <DialogHeader>
 <DialogTitle>
 {editingRole ? 'Edit Role' : 'Create Role'}
 </DialogTitle>
 </DialogHeader>

 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="role-name">Name *</Label>
 <Input
 id="role-name"
 value={formName}
 onChange={(e) => setFormName(e.target.value)}
 placeholder="e.g. Manager"
 />
 </div>

 <div className="space-y-2">
 <Label htmlFor="role-description">Description</Label>
 <Textarea
 id="role-description"
 value={formDescription}
 onChange={(e) => setFormDescription(e.target.value)}
 placeholder="Optional description of this role"
 rows={3}
 />
 </div>

 {!editingRole && (
 <div className="flex items-center gap-2">
 <Checkbox
 id="role-is-system"
 checked={formIsSystem}
 onCheckedChange={(checked) =>
 setFormIsSystem(checked === true)
 }
 />
 <Label htmlFor="role-is-system" className="text-sm font-normal">
 System role
 </Label>
 </div>
 )}
 </div>

 <DialogFooter>
 <Button
 variant="outline"
 onClick={() => setDialogOpen(false)}
 disabled={saving}
 >
 Cancel
 </Button>
 <Button onClick={handleSave} disabled={saving}>
 {saving && <Loader2 className="h-4 w-4 mr-1 animate-spin" />}
 {editingRole ? 'Save Changes' : 'Create'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
