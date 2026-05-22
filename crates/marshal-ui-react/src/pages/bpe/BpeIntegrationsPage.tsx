import { useState, useEffect, useCallback } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
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
import {
 Dialog,
 DialogContent,
 DialogDescription,
 DialogFooter,
 DialogHeader,
 DialogTitle,
} from '@/components/ui/dialog'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import { Loader2, Plug, CheckCircle, XCircle, RefreshCw, Trash2, TestTube, Plus } from 'lucide-react'
import type { IntegrationCredential, IntegrationType } from '@/models/bpe'

const NON_SECRET_FIELDS = new Set([
 'url', 'base_url', 'host', 'port', 'from', 'email', 'owner', 'repo', 'auth_type',
 'realm_id', 'account_id', 'client_id',
])

export function BpeIntegrationsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [types, setTypes] = useState<IntegrationType[]>([])
 const [credentials, setCredentials] = useState<IntegrationCredential[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [testing, setTesting] = useState<string | null>(null)
 const [testResult, setTestResult] = useState<{ id: string; success: boolean; message: string } | null>(null)

 // Create dialog state
 const [createOpen, setCreateOpen] = useState(false)
 const [createType, setCreateType] = useState('')
 const [createName, setCreateName] = useState('')
 const [createFields, setCreateFields] = useState<Record<string, string>>({})
 const [creating, setCreating] = useState(false)

 // Delete confirm state
 const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null)

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const [t, c] = await Promise.all([
 client.listIntegrationTypes(),
 client.listCredentials(orgSlug),
 ])
 setTypes(t.data)
 setCredentials(c.data)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load integrations')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 const selectedType = types.find((t) => t.name === createType)
 const credentialFields = selectedType?.credential_fields ?? []

 const resetCreateForm = () => {
 setCreateType('')
 setCreateName('')
 setCreateFields({})
 }

 const handleCreateOpen = (typeName?: string) => {
 resetCreateForm()
 if (typeName) setCreateType(typeName)
 setCreateOpen(true)
 }

 const handleCreate = async () => {
 if (!token || !orgSlug || !createType || !createName.trim()) return
 setCreating(true)
 try {
 const client = new BpeClient(token)
 await client.createCredential({
 organization_id: orgSlug,
 integration_type: createType,
 name: createName.trim(),
 credentials: createFields,
 })
 toast.success('Credential created successfully')
 setCreateOpen(false)
 resetCreateForm()
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to create credential')
 } finally {
 setCreating(false)
 }
 }

 const testCred = async (id: string) => {
 if (!token) return
 setTesting(id)
 setTestResult(null)
 try {
 const client = new BpeClient(token)
 const res = await client.testCredential(id)
 setTestResult({ id, ...res })
 if (res.success) {
 toast.success('Connection test passed')
 } else {
 toast.error(res.message || 'Connection test failed')
 }
 await fetchData()
 } catch (err) {
 const message = err instanceof Error ? err.message : 'Test failed'
 setTestResult({ id, success: false, message })
 toast.error(message)
 } finally {
 setTesting(null)
 }
 }

 const deleteCred = async (id: string) => {
 if (!token) return
 try {
 const client = new BpeClient(token)
 await client.deleteCredential(id)
 toast.success('Credential deleted')
 await fetchData()
 } catch (err) {
 const message = err instanceof Error ? err.message : 'Delete failed'
 setError(message)
 toast.error(message)
 }
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view integrations.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Integrations</h1>
 <div className="flex gap-2">
 <Button size="sm" onClick={() => handleCreateOpen()}>
 <Plus className="w-4 h-4 mr-2" />Add Credential
 </Button>
 <Button variant="outline" size="sm" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Refresh</Button>
 </div>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Available Integration Types */}
 <div>
 <h2 className="text-sm font-semibold text-gray-500 uppercase tracking-wider mb-3">Available Types</h2>
 <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
 {types.map((t) => (
 <Card key={t.name} className="hover:shadow-sm transition-shadow cursor-pointer" onClick={() => handleCreateOpen(t.name)}>
 <CardContent className="pt-3 pb-3 text-center">
 <Plug className="w-6 h-6 mx-auto text-indigo-500 mb-2" />
 <p className="text-sm font-medium text-gray-900">{t.display_name}</p>
 <p className="text-xs text-gray-400 mt-1">{t.description}</p>
 <p className="text-xs text-gray-400 mt-0.5">{t.credential_fields.length} fields</p>
 </CardContent>
 </Card>
 ))}
 </div>
 </div>

 {/* Credentials */}
 <div>
 <h2 className="text-sm font-semibold text-gray-500 uppercase tracking-wider mb-3">Configured Credentials</h2>
 {credentials.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-8">No credentials configured</p>
 ) : (
 <div className="space-y-3">
 {credentials.map((cred) => (
 <Card key={cred.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <Badge variant="secondary">{cred.integration_type}</Badge>
 <span className="font-medium text-gray-900">{cred.name}</span>
 <Badge variant={cred.is_active ? 'default' : 'outline'}>
 {cred.is_active ? 'Active' : 'Inactive'}
 </Badge>
 {cred.last_tested_at && (
 <span className="text-xs text-gray-400 flex items-center gap-1">
 {cred.last_test_success ? (
 <CheckCircle className="w-3 h-3 text-emerald-500" />
 ) : (
 <XCircle className="w-3 h-3 text-red-500" />
 )}
 Tested {new Date(cred.last_tested_at).toLocaleDateString()}
 </span>
 )}
 </div>
 <div className="flex gap-2">
 <Button
 size="sm"
 variant="outline"
 onClick={() => testCred(cred.id)}
 disabled={testing === cred.id}
 >
 {testing === cred.id ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <TestTube className="w-3.5 h-3.5 mr-1" />}
 Test
 </Button>
 <Button size="sm" variant="ghost" onClick={() => setDeleteTarget({ id: cred.id, name: cred.name })}>
 <Trash2 className="w-3.5 h-3.5 text-red-500" />
 </Button>
 </div>
 </div>
 {testResult && testResult.id === cred.id && (
 <div className={`mt-2 text-sm px-3 py-2 rounded ${testResult.success ? 'bg-emerald-50 text-emerald-700' : 'bg-red-50 text-red-700'}`}>
 {testResult.message}
 </div>
 )}
 </CardContent>
 </Card>
 ))}
 </div>
 )}
 </div>

 {/* Create Credential Dialog */}
 <Dialog open={createOpen} onOpenChange={(open) => { setCreateOpen(open); if (!open) resetCreateForm() }}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>Add Credential</DialogTitle>
 <DialogDescription>Configure a new integration credential.</DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="cred-type">Integration Type</Label>
 <Select value={createType} onValueChange={(v) => { setCreateType(v); setCreateFields({}) }}>
 <SelectTrigger id="cred-type">
 <SelectValue placeholder="Select type..." />
 </SelectTrigger>
 <SelectContent>
 {types.map((t) => (
 <SelectItem key={t.name} value={t.name}>{t.display_name}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 <div className="space-y-2">
 <Label htmlFor="cred-name">Name</Label>
 <Input
 id="cred-name"
 placeholder="e.g. Production Slack"
 value={createName}
 onChange={(e) => setCreateName(e.target.value)}
 />
 </div>
 {credentialFields.length > 0 && (
 <div className="space-y-3 border-t pt-3">
 <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Credential Fields</p>
 {credentialFields.map((field) => {
 const isSecret = !NON_SECRET_FIELDS.has(field)
 return (
 <div key={field} className="space-y-1">
 <Label htmlFor={`cred-field-${field}`}>
 {field}
 </Label>
 <Input
 id={`cred-field-${field}`}
 type={isSecret ? 'password' : 'text'}
 placeholder={field}
 value={createFields[field] ?? ''}
 onChange={(e) => setCreateFields((prev) => ({ ...prev, [field]: e.target.value }))}
 />
 </div>
 )
 })}
 </div>
 )}
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>Cancel</Button>
 <Button
 onClick={handleCreate}
 disabled={creating || !createType || !createName.trim()}
 >
 {creating ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
 Create
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete Confirm Dialog */}
 <ConfirmDialog
 open={!!deleteTarget}
 onOpenChange={(open) => { if (!open) setDeleteTarget(null) }}
 title="Delete Credential"
 description={`Are you sure you want to delete "${deleteTarget?.name ?? ''}"? This action cannot be undone.`}
 confirmLabel="Delete"
 variant="danger"
 onConfirm={async () => {
 if (deleteTarget) await deleteCred(deleteTarget.id)
 setDeleteTarget(null)
 }}
 />
 </div>
 )
}
