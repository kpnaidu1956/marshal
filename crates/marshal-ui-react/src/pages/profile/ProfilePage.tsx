import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { Switch } from '@/components/ui/switch'
import { Badge } from '@/components/ui/badge'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import { Pencil, Save, X, Lock } from 'lucide-react'

interface ProfileFormData {
 first_name: string
 last_name: string
 username: string
 mobile_phone: string
 badge_number: string
 title: string
 level: string
}

const LEVEL_OPTIONS = ['Entry', 'Mid', 'Senior', 'Lead', 'Director'] as const

export function ProfilePage() {
 const user = useAuthStore((s) => s.user)
 const token = useAuthStore((s) => s.token)
 const currentOrg = useOrgStore((s) => s.currentOrg)

 const [editing, setEditing] = useState(false)
 const [saving, setSaving] = useState(false)
 const [formData, setFormData] = useState<ProfileFormData>({
 first_name: '',
 last_name: '',
 username: '',
 mobile_phone: '',
 badge_number: '',
 title: '',
 level: '',
 })

 // Password change state
 const [changingPassword, setChangingPassword] = useState(false)
 const [savingPassword, setSavingPassword] = useState(false)
 const [currentPassword, setCurrentPassword] = useState('')
 const [newPassword, setNewPassword] = useState('')
 const [confirmPassword, setConfirmPassword] = useState('')


 if (!user) return <p className="text-sm text-muted-foreground">Not logged in.</p>

 const displayName =
 [user.first_name, user.last_name].filter(Boolean).join(' ') || user.email || 'User'
 const initials = [user.first_name, user.last_name]
 .filter(Boolean)
 .map((n) => n!.charAt(0).toUpperCase())
 .join('')
 || (user.email ? user.email.charAt(0).toUpperCase() : 'U')

 const orgName = currentOrg?.display_name ?? currentOrg?.name ?? '--'

 function startEditing() {
 setFormData({
 first_name: user!.first_name ?? '',
 last_name: user!.last_name ?? '',
 username: user!.username ?? '',
 mobile_phone: user!.mobile_phone ?? '',
 badge_number: user!.badge_number ?? '',
 title: user!.title ?? '',
 level: user!.level ?? '',
 })
 setEditing(true)
 }

 function cancelEditing() {
 setEditing(false)
 }

 function updateField(field: keyof ProfileFormData, value: string) {
 setFormData((prev) => ({ ...prev, [field]: value }))
 }

 async function handleSave() {
 if (!user || !token) return

 setSaving(true)
 try {
 const { postgrestUrl, apiKey } = detectApiUrls()
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = new QueryBuilder().eq('id', user.id).build()

 const updatedFields = {
 first_name: formData.first_name || null,
 last_name: formData.last_name || null,
 username: formData.username || null,
 mobile_phone: formData.mobile_phone || null,
 badge_number: formData.badge_number || null,
 title: formData.title || null,
 level: formData.level || null,
 }

 await client.patch('users', qs, updatedFields, token)

 useAuthStore.getState().login(token, { ...user, ...updatedFields })
 setEditing(false)
 toast.success('Profile updated successfully')
 } catch (err) {
 toast.error('Failed to update profile: ' + (err instanceof Error ? err.message : String(err)))
 } finally {
 setSaving(false)
 }
 }

 async function handlePasswordChange() {
 if (!token) return

 if (newPassword !== confirmPassword) {
 toast.error('New passwords do not match')
 return
 }
 if (newPassword.length < 8) {
 toast.error('Password must be at least 8 characters')
 return
 }
 if (newPassword.length > 72) {
 toast.error('Password must not exceed 72 characters')
 return
 }

 setSavingPassword(true)
 try {
 const { ragUrl } = detectApiUrls()
 const resp = await fetch(ragUrl + '/api/auth/set-password', {
 method: 'POST',
 headers: {
 'Content-Type': 'application/json',
 Authorization: 'Bearer ' + token,
 },
 body: JSON.stringify({ password: newPassword }),
 })

 if (!resp.ok) {
 const text = await resp.text()
 throw new Error(text || `HTTP ${resp.status}`)
 }

 setCurrentPassword('')
 setNewPassword('')
 setConfirmPassword('')
 setChangingPassword(false)
 toast.success('Password changed successfully')
 } catch (err) {
 toast.error('Failed to change password: ' + (err instanceof Error ? err.message : String(err)))
 } finally {
 setSavingPassword(false)
 }
 }

 function renderField(label: string, value: string | null | undefined) {
 return <span className="text-sm text-foreground">{value || '--'}</span>
 }

 return (
 <div className="max-w-2xl mx-auto space-y-6">
 {/* Avatar + Name Header */}
 <div className="flex items-center gap-4">
 <div className="w-20 h-20 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-2xl font-bold shrink-0">
 {initials}
 </div>
 <div className="min-w-0">
 <h1 className="text-2xl font-bold text-foreground">{displayName}</h1>
 {user.title && <p className="text-sm text-muted-foreground">{user.title}</p>}
 {user.is_platform_admin && (
 <Badge variant="secondary" className="mt-1">Admin</Badge>
 )}
 </div>
 <div className="ml-auto">
 {!editing ? (
 <Button variant="outline" size="sm" onClick={startEditing}>
 <Pencil className="h-4 w-4 mr-2" />
 Edit Profile
 </Button>
 ) : (
 <div className="flex gap-2">
 <Button size="sm" onClick={handleSave} disabled={saving}>
 <Save className="h-4 w-4 mr-2" />
 {saving ? 'Saving...' : 'Save'}
 </Button>
 <Button variant="outline" size="sm" onClick={cancelEditing} disabled={saving}>
 <X className="h-4 w-4 mr-2" />
 Cancel
 </Button>
 </div>
 )}
 </div>
 </div>

 {/* Personal Information */}
 <Card>
 <CardHeader>
 <CardTitle className="text-lg">Personal Information</CardTitle>
 </CardHeader>
 <CardContent className="space-y-4">
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-2">
 <Label htmlFor="first_name">First Name</Label>
 {editing ? (
 <Input
 id="first_name"
 value={formData.first_name}
 onChange={(e) => updateField('first_name', e.target.value)}
 />
 ) : (
 renderField('First Name', user.first_name)
 )}
 </div>
 <div className="space-y-2">
 <Label htmlFor="last_name">Last Name</Label>
 {editing ? (
 <Input
 id="last_name"
 value={formData.last_name}
 onChange={(e) => updateField('last_name', e.target.value)}
 />
 ) : (
 renderField('Last Name', user.last_name)
 )}
 </div>
 </div>

 <div className="space-y-2">
 <Label htmlFor="email">Email</Label>
 <span className="block text-sm text-muted-foreground">{user.email || '--'}</span>
 </div>

 <div className="space-y-2">
 <Label htmlFor="username">Username</Label>
 {editing ? (
 <Input
 id="username"
 value={formData.username}
 onChange={(e) => updateField('username', e.target.value)}
 />
 ) : (
 renderField('Username', user.username)
 )}
 </div>

 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-2">
 <Label htmlFor="mobile_phone">Phone</Label>
 {editing ? (
 <Input
 id="mobile_phone"
 value={formData.mobile_phone}
 onChange={(e) => updateField('mobile_phone', e.target.value)}
 placeholder="+1234567890"
 />
 ) : (
 renderField('Phone', user.mobile_phone)
 )}
 </div>
 <div className="space-y-2">
 <Label htmlFor="badge_number">Badge Number</Label>
 {editing ? (
 <Input
 id="badge_number"
 value={formData.badge_number}
 onChange={(e) => updateField('badge_number', e.target.value)}
 />
 ) : (
 renderField('Badge Number', user.badge_number)
 )}
 </div>
 </div>
 </CardContent>
 </Card>

 {/* Role & Organization */}
 <Card>
 <CardHeader>
 <CardTitle className="text-lg">Role & Organization</CardTitle>
 </CardHeader>
 <CardContent className="space-y-4">
 <div className="space-y-2">
 <Label htmlFor="title">Title</Label>
 {editing ? (
 <Input
 id="title"
 value={formData.title}
 onChange={(e) => updateField('title', e.target.value)}
 />
 ) : (
 renderField('Title', user.title)
 )}
 </div>

 <div className="space-y-2">
 <Label htmlFor="level">Level</Label>
 {editing ? (
 <Select
 value={formData.level}
 onValueChange={(v) => updateField('level', v)}
 >
 <SelectTrigger id="level">
 <SelectValue placeholder="Select level" />
 </SelectTrigger>
 <SelectContent>
 {LEVEL_OPTIONS.map((lvl) => (
 <SelectItem key={lvl} value={lvl}>{lvl}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 ) : (
 renderField('Level', user.level)
 )}
 </div>

 <div className="space-y-2">
 <Label>Organization</Label>
 <span className="block text-sm text-muted-foreground">{orgName}</span>
 </div>
 </CardContent>
 </Card>

 {/* Password Change */}
 <Card>
 <CardHeader>
 <CardTitle className="text-lg flex items-center gap-2">
 <Lock className="h-4 w-4" />
 Change Password
 </CardTitle>
 </CardHeader>
 <CardContent>
 {!changingPassword ? (
 <Button variant="outline" size="sm" onClick={() => setChangingPassword(true)}>
 Change Password
 </Button>
 ) : (
 <div className="space-y-4">
 <div className="space-y-2">
 <Label htmlFor="current_password">Current Password</Label>
 <Input
 id="current_password"
 type="password"
 value={currentPassword}
 onChange={(e) => setCurrentPassword(e.target.value)}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="new_password">New Password</Label>
 <Input
 id="new_password"
 type="password"
 value={newPassword}
 onChange={(e) => setNewPassword(e.target.value)}
 placeholder="8-72 characters"
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="confirm_password">Confirm New Password</Label>
 <Input
 id="confirm_password"
 type="password"
 value={confirmPassword}
 onChange={(e) => setConfirmPassword(e.target.value)}
 />
 </div>
 <div className="flex gap-2">
 <Button size="sm" onClick={handlePasswordChange} disabled={savingPassword}>
 {savingPassword ? 'Saving...' : 'Update Password'}
 </Button>
 <Button
 variant="outline"
 size="sm"
 onClick={() => {
 setChangingPassword(false)
 setCurrentPassword('')
 setNewPassword('')
 setConfirmPassword('')
 }}
 disabled={savingPassword}
 >
 Cancel
 </Button>
 </div>
 </div>
 )}
 </CardContent>
 </Card>

 </div>
 )
}
