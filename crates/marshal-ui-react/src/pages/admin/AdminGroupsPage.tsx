import { useState, useEffect, useCallback, useMemo } from 'react'
import {
 Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog'
import {
 AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent,
 AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import { Loader2, Plus, Pencil, Trash2, FolderOpen, Users, Shield, UserPlus, X, Check } from 'lucide-react'

const FEATURES = ['timekeeping', 'roster', 'reports', 'audit', 'approvals', 'admin', 'knowledge', 'marshal', 'analytics']
const ACTIONS = ['read', 'write', 'delete', 'admin']

interface GroupItem { id: string; name: string; description: string | null; member_count: number; created_at: string }
interface MemberItem { user_id: string; first_name: string; last_name: string; email: string | null; title: string | null }
interface PermItem { id: string; group_id: string; feature: string; action: string }
interface UserOption { id: string; first_name: string; last_name: string; email: string | null }

export function AdminGroupsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug ?? '')
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')

 const [groups, setGroups] = useState<GroupItem[]>([])
 const [loading, setLoading] = useState(true)
 const [dialogOpen, setDialogOpen] = useState(false)
 const [editingGroup, setEditingGroup] = useState<GroupItem | null>(null)
 const [saving, setSaving] = useState(false)
 const [formName, setFormName] = useState('')
 const [formDescription, setFormDescription] = useState('')

 // Detail panel
 const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null)
 const [members, setMembers] = useState<MemberItem[]>([])
 const [perms, setPerms] = useState<PermItem[]>([])
 const [detailLoading, setDetailLoading] = useState(false)
 const [allUsers, setAllUsers] = useState<UserOption[]>([])
 const [addMemberUserId, setAddMemberUserId] = useState('')

 const client = useMemo(() => token ? new BpeClient(token) : null, [token])

 const fetchGroups = useCallback(async () => {
 if (!client || !orgSlug) return
 setLoading(true)
 try {
  const res = await client.listGroups(orgSlug)
  setGroups(res.data)
 } catch { toast.error('Failed to load groups') }
 finally { setLoading(false) }
 }, [client, orgSlug])

 const fetchUsers = useCallback(async () => {
 if (!orgId) return
 const { postgrestUrl, apiKey } = detectApiUrls()
 const pg = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().select('id,first_name,last_name,email').eq('organization_id', orgId).order('first_name', true).limit(200).build()
 const data = await pg.get<UserOption>('users', qs, token)
 setAllUsers(data)
 }, [orgId, token])

 useEffect(() => { fetchGroups(); fetchUsers() }, [fetchGroups, fetchUsers])

 const fetchDetail = useCallback(async (groupId: string) => {
 if (!client || !orgSlug) return
 setDetailLoading(true)
 try {
  const [m, p] = await Promise.all([
  client.listGroupMembers(groupId, orgSlug),
  client.listGroupPermissions(groupId, orgSlug),
  ])
  setMembers(m.data)
  setPerms(p.data)
 } catch { toast.error('Failed to load group details') }
 finally { setDetailLoading(false) }
 }, [client, orgSlug])

 useEffect(() => {
 if (selectedGroupId) fetchDetail(selectedGroupId)
 else { setMembers([]); setPerms([]) }
 }, [selectedGroupId, fetchDetail])

 const selectedGroup = groups.find((g) => g.id === selectedGroupId)

 function openCreate() { setEditingGroup(null); setFormName(''); setFormDescription(''); setDialogOpen(true) }
 function openEdit(g: GroupItem) { setEditingGroup(g); setFormName(g.name); setFormDescription(g.description ?? ''); setDialogOpen(true) }

 async function handleSave() {
 if (!formName.trim() || !client) return
 setSaving(true)
 try {
  if (editingGroup) {
  await client.updateGroup(editingGroup.id, { name: formName.trim(), description: formDescription.trim() || undefined })
  toast.success('Group updated')
  } else {
  await client.createGroup({ organization_id: orgSlug, name: formName.trim(), description: formDescription.trim() || undefined })
  toast.success('Group created')
  }
  setDialogOpen(false); fetchGroups()
 } catch { toast.error('Failed to save group') }
 finally { setSaving(false) }
 }

 async function handleDelete(g: GroupItem) {
 if (!client) return
 try { await client.deleteGroup(g.id, orgSlug); toast.success('Group deleted'); if (selectedGroupId === g.id) setSelectedGroupId(null); fetchGroups() }
 catch { toast.error('Failed to delete group') }
 }

 async function handleAddMember() {
 if (!client || !addMemberUserId || !selectedGroupId) return
 try { await client.addGroupMember(selectedGroupId, { organization_id: orgSlug, user_id: addMemberUserId }); toast.success('Member added'); setAddMemberUserId(''); fetchDetail(selectedGroupId); fetchGroups() }
 catch { toast.error('Failed to add member') }
 }

 async function handleRemoveMember(userId: string) {
 if (!client || !selectedGroupId) return
 try { await client.removeGroupMember(selectedGroupId, userId, orgSlug); toast.success('Member removed'); fetchDetail(selectedGroupId); fetchGroups() }
 catch { toast.error('Failed to remove member') }
 }

 async function handleTogglePerm(feature: string, action: string) {
 if (!client || !selectedGroupId) return
 const existing = perms.find((p) => p.feature === feature && p.action === action)
 try {
  if (existing) {
  await client.removeGroupPermission(selectedGroupId, existing.id, orgSlug)
  } else {
  await client.addGroupPermission(selectedGroupId, { organization_id: orgSlug, feature, action })
  }
  fetchDetail(selectedGroupId)
 } catch { toast.error('Failed to update permission') }
 }

 const memberIds = new Set(members.map((m) => m.user_id))
 const availableUsers = allUsers.filter((u) => !memberIds.has(u.id))

 return (
 <div className="space-y-4">
  <div className="flex items-center justify-between">
  <div className="flex items-center gap-3">
   <FolderOpen className="h-6 w-6 text-primary" />
   <h1 className="text-2xl font-bold">Groups</h1>
   {!loading && <Badge variant="secondary">{groups.length}</Badge>}
  </div>
  <Button size="sm" onClick={openCreate}><Plus className="mr-1 h-4 w-4" />Add Group</Button>
  </div>

  <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
  {/* Left: Group list */}
  <Card className="lg:col-span-1">
   <CardContent className="p-0">
   {loading ? (
    <div className="flex items-center justify-center py-12"><Loader2 className="h-5 w-5 animate-spin" /></div>
   ) : groups.length === 0 ? (
    <div className="py-12 text-center text-sm text-muted-foreground">No groups yet</div>
   ) : (
    <div className="divide-y">
    {groups.map((g) => (
     <div key={g.id} className={`px-4 py-3 cursor-pointer hover:bg-muted/50 transition-colors ${selectedGroupId === g.id ? 'bg-muted' : ''}`} onClick={() => setSelectedGroupId(g.id)}>
     <div className="flex items-center justify-between">
      <div>
      <p className="text-sm font-medium">{g.name}</p>
      <p className="text-xs text-muted-foreground">{g.member_count} members</p>
      </div>
      <div className="flex items-center gap-1">
      <Button variant="ghost" size="icon" className="h-7 w-7" onClick={(e) => { e.stopPropagation(); openEdit(g) }}><Pencil className="h-3 w-3" /></Button>
      <AlertDialog>
       <AlertDialogTrigger asChild>
       <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive" onClick={(e) => e.stopPropagation()}><Trash2 className="h-3 w-3" /></Button>
       </AlertDialogTrigger>
       <AlertDialogContent>
       <AlertDialogHeader><AlertDialogTitle>Delete "{g.name}"?</AlertDialogTitle><AlertDialogDescription>This will remove all members and permissions.</AlertDialogDescription></AlertDialogHeader>
       <AlertDialogFooter><AlertDialogCancel>Cancel</AlertDialogCancel><AlertDialogAction onClick={() => handleDelete(g)}>Delete</AlertDialogAction></AlertDialogFooter>
       </AlertDialogContent>
      </AlertDialog>
      </div>
     </div>
     </div>
    ))}
    </div>
   )}
   </CardContent>
  </Card>

  {/* Right: Detail panel */}
  <Card className="lg:col-span-2">
   {!selectedGroupId ? (
   <CardContent className="flex flex-col items-center justify-center py-16 text-muted-foreground">
    <Users className="h-10 w-10 mb-3 opacity-50" />
    <p className="text-sm">Select a group to manage members and permissions</p>
   </CardContent>
   ) : detailLoading ? (
   <CardContent className="flex items-center justify-center py-16"><Loader2 className="h-5 w-5 animate-spin" /></CardContent>
   ) : (
   <CardContent className="p-4 space-y-6">
    {/* Members section */}
    <div>
    <div className="flex items-center justify-between mb-3">
     <h3 className="text-sm font-semibold flex items-center gap-2"><Users className="h-4 w-4" />Members ({members.length})</h3>
    </div>
    <div className="flex items-center gap-2 mb-3">
     <Select value={addMemberUserId} onValueChange={setAddMemberUserId}>
     <SelectTrigger className="w-64"><SelectValue placeholder="Add a member..." /></SelectTrigger>
     <SelectContent>
      {availableUsers.map((u) => (
      <SelectItem key={u.id} value={u.id}>{u.first_name} {u.last_name}</SelectItem>
      ))}
     </SelectContent>
     </Select>
     <Button size="sm" onClick={handleAddMember} disabled={!addMemberUserId}><UserPlus className="mr-1 h-4 w-4" />Add</Button>
    </div>
    {members.length === 0 ? (
     <p className="text-xs text-muted-foreground">No members yet</p>
    ) : (
     <div className="space-y-1">
     {members.map((m) => (
      <div key={m.user_id} className="flex items-center justify-between p-2 rounded-lg bg-muted/30">
      <div>
       <span className="text-sm font-medium">{m.first_name} {m.last_name}</span>
       {m.title && <span className="text-xs text-muted-foreground ml-2">({m.title})</span>}
      </div>
      <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive" onClick={() => handleRemoveMember(m.user_id)}><X className="h-3 w-3" /></Button>
      </div>
     ))}
     </div>
    )}
    </div>

    <Separator />

    {/* Permissions grid */}
    <div>
    <h3 className="text-sm font-semibold flex items-center gap-2 mb-3"><Shield className="h-4 w-4" />Feature Permissions</h3>
    <div className="overflow-x-auto">
     <table className="w-full text-xs">
     <thead>
      <tr className="border-b bg-muted/50">
      <th className="px-2 py-2 text-left font-semibold uppercase">Feature</th>
      {ACTIONS.map((a) => <th key={a} className="px-2 py-2 text-center font-semibold uppercase">{a}</th>)}
      </tr>
     </thead>
     <tbody>
      {FEATURES.map((f) => (
      <tr key={f} className="border-b last:border-0 hover:bg-muted/30">
       <td className="px-2 py-2 font-medium capitalize">{f}</td>
       {ACTIONS.map((a) => {
       const has = perms.some((p) => p.feature === f && p.action === a)
       return (
        <td key={a} className="px-2 py-2 text-center">
        <button onClick={() => handleTogglePerm(f, a)} className={`w-6 h-6 rounded border transition-colors ${has ? 'bg-primary border-primary text-white' : 'border-gray-300 hover:border-primary'}`}>
         {has && <Check className="h-4 w-4 mx-auto" />}
        </button>
        </td>
       )
       })}
      </tr>
      ))}
     </tbody>
     </table>
    </div>
    </div>
   </CardContent>
   )}
  </Card>
  </div>

  {/* Create/Edit Dialog */}
  <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
  <DialogContent className="sm:max-w-md">
   <DialogHeader><DialogTitle>{editingGroup ? 'Edit Group' : 'Create Group'}</DialogTitle></DialogHeader>
   <div className="space-y-4 py-2">
   <div className="space-y-2">
    <Label>Name *</Label>
    <Input placeholder="Group name" value={formName} onChange={(e) => setFormName(e.target.value)} autoFocus />
   </div>
   <div className="space-y-2">
    <Label>Description</Label>
    <Textarea placeholder="Optional" value={formDescription} onChange={(e) => setFormDescription(e.target.value)} rows={3} />
   </div>
   </div>
   <DialogFooter>
   <Button variant="outline" onClick={() => setDialogOpen(false)} disabled={saving}>Cancel</Button>
   <Button onClick={handleSave} disabled={saving}>{saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}{editingGroup ? 'Save' : 'Create'}</Button>
   </DialogFooter>
  </DialogContent>
  </Dialog>
 </div>
 )
}
