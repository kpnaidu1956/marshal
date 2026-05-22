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
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import { Loader2, Plus, Pencil, Trash2, Building2 } from 'lucide-react'
import type { Organization } from '@/models/organization'

interface OrgFormData {
 name: string
 display_name: string
 description: string
 logo_url: string
}

const emptyForm: OrgFormData = {
 name: '',
 display_name: '',
 description: '',
 logo_url: '',
}

export function AdminOrganizationsPage() {
 const token = useAuthStore((s) => s.token)
 const user = useAuthStore((s) => s.user)
 const { postgrestUrl, apiKey } = detectApiUrls()

 const [orgs, setOrgs] = useState<Organization[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Dialog state
 const [dialogOpen, setDialogOpen] = useState(false)
 const [editingOrg, setEditingOrg] = useState<Organization | null>(null)
 const [formData, setFormData] = useState<OrgFormData>(emptyForm)
 const [saving, setSaving] = useState(false)

 const client = new PostgRestClient(postgrestUrl, apiKey)

 const fetchOrgs = useCallback(async () => {
 setLoading(true)
 setError(null)
 try {
 const qs = new QueryBuilder()
 .select('id,name,display_name,description,logo_url,created_by,created_at')
 .order('name', true)
 .build()
 const data = await client.get<Organization>('organizations', qs, token)
 setOrgs(data)
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to load organizations'
 setError(msg)
 } finally {
 setLoading(false)
 }
 // eslint-disable-next-line react-hooks/exhaustive-deps
 }, [postgrestUrl, apiKey, token])

 useEffect(() => {
 fetchOrgs()
 }, [fetchOrgs])

 function openCreate() {
 setEditingOrg(null)
 setFormData(emptyForm)
 setDialogOpen(true)
 }

 function openEdit(org: Organization) {
 setEditingOrg(org)
 setFormData({
 name: org.name,
 display_name: org.display_name ?? '',
 description: org.description ?? '',
 logo_url: org.logo_url ?? '',
 })
 setDialogOpen(true)
 }

 async function handleSave() {
 if (!formData.name.trim()) {
 toast.error('Name is required')
 return
 }

 setSaving(true)
 try {
 const payload: Record<string, unknown> = {
 name: formData.name.trim(),
 display_name: formData.display_name.trim() || null,
 description: formData.description.trim() || null,
 logo_url: formData.logo_url.trim() || null,
 }

 if (editingOrg) {
 const qs = new QueryBuilder().eq('id', editingOrg.id).build()
 await client.patch<Organization>('organizations', qs, payload, token)
 toast.success('Organization updated')
 } else {
 payload.created_by = user?.id ?? null
 await client.post<Organization>('organizations', payload, token)
 toast.success('Organization created')
 }

 setDialogOpen(false)
 await fetchOrgs()
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to save organization'
 toast.error(msg)
 } finally {
 setSaving(false)
 }
 }

 async function handleDelete(org: Organization) {
 try {
 const qs = new QueryBuilder().eq('id', org.id).build()
 await client.delete('organizations', qs, token)
 toast.success(`Deleted "${org.display_name || org.name}"`)
 await fetchOrgs()
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Failed to delete organization'
 toast.error(msg)
 }
 }

 return (
 <div className="space-y-4">
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
 <div className="flex items-center gap-3">
 <Building2 className="h-5 w-5 text-muted-foreground" />
 <CardTitle className="text-xl font-semibold text-foreground">
 Organizations
 </CardTitle>
 {!loading && (
 <Badge variant="secondary">{orgs.length}</Badge>
 )}
 </div>
 <Button size="sm" onClick={openCreate}>
 <Plus className="mr-1 h-4 w-4" />
 Add Organization
 </Button>
 </CardHeader>

 <CardContent>
 {error && (
 <p className="mb-4 text-sm text-destructive">{error}</p>
 )}

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
 Display Name
 </th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">
 Description
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
 {orgs.map((o) => (
 <tr
 key={o.id}
 className="border-b last:border-0 hover:bg-muted/50"
 >
 <td className="px-4 py-3 text-sm font-medium text-foreground">
 {o.name}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {o.display_name ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground max-w-xs truncate">
 {o.description ?? '--'}
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground">
 {o.created_at?.slice(0, 10) ?? '--'}
 </td>
 <td className="px-4 py-3 text-right">
 <div className="flex items-center justify-end gap-1">
 <Button
 variant="ghost"
 size="icon"
 onClick={() => openEdit(o)}
 title="Edit"
 >
 <Pencil className="h-4 w-4" />
 </Button>

 <AlertDialog>
 <AlertDialogTrigger asChild>
 <Button
 variant="ghost"
 size="icon"
 title="Delete"
 >
 <Trash2 className="h-4 w-4 text-destructive" />
 </Button>
 </AlertDialogTrigger>
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>
 Delete Organization
 </AlertDialogTitle>
 <AlertDialogDescription>
 Are you sure you want to delete &ldquo;
 {o.display_name || o.name}&rdquo;? This action
 cannot be undone.
 </AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel>Cancel</AlertDialogCancel>
 <AlertDialogAction
 onClick={() => handleDelete(o)}
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
 {orgs.length === 0 && (
 <tr>
 <td
 colSpan={5}
 className="px-4 py-8 text-center text-sm text-muted-foreground"
 >
 No organizations found.
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
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>
 {editingOrg ? 'Edit Organization' : 'Add Organization'}
 </DialogTitle>
 </DialogHeader>

 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="org-name">
 Name <span className="text-destructive">*</span>
 </Label>
 <Input
 id="org-name"
 placeholder="e.g. acme-corp"
 value={formData.name}
 onChange={(e) =>
 setFormData((f) => ({ ...f, name: e.target.value }))
 }
 />
 </div>

 <div className="space-y-2">
 <Label htmlFor="org-display-name">Display Name</Label>
 <Input
 id="org-display-name"
 placeholder="e.g. ACME Corporation"
 value={formData.display_name}
 onChange={(e) =>
 setFormData((f) => ({ ...f, display_name: e.target.value }))
 }
 />
 </div>

 <div className="space-y-2">
 <Label htmlFor="org-description">Description</Label>
 <Textarea
 id="org-description"
 placeholder="Organization description..."
 rows={3}
 value={formData.description}
 onChange={(e) =>
 setFormData((f) => ({ ...f, description: e.target.value }))
 }
 />
 </div>

 <div className="space-y-2">
 <Label htmlFor="org-logo">Logo URL</Label>
 <Input
 id="org-logo"
 placeholder="https://..."
 value={formData.logo_url}
 onChange={(e) =>
 setFormData((f) => ({ ...f, logo_url: e.target.value }))
 }
 />
 </div>
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
 {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
 {editingOrg ? 'Save Changes' : 'Create'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
