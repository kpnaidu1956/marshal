import { useState, useEffect, useCallback } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import {
 Dialog,
 DialogContent,
 DialogHeader,
 DialogTitle,
 DialogDescription,
 DialogFooter,
} from '@/components/ui/dialog'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import {
 Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'
import { Loader2, Database, RefreshCw, Plus, Trash2, Search, X } from 'lucide-react'
import type { BpeEntityType, BpeEntity } from '@/models/bpe'

export function BpeEntitiesPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [tab, setTab] = useState<'entities' | 'types'>('entities')
 const [entityTypes, setEntityTypes] = useState<BpeEntityType[]>([])
 const [entities, setEntities] = useState<BpeEntity[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Create Entity Type dialog state
 const [showCreateType, setShowCreateType] = useState(false)
 const [createTypeName, setCreateTypeName] = useState('')
 const [createTypeSchema, setCreateTypeSchema] = useState('{}')
 const [creatingType, setCreatingType] = useState(false)

 // Create Entity dialog state
 const [showCreateEntity, setShowCreateEntity] = useState(false)
 const [createEntityName, setCreateEntityName] = useState('')
 const [createEntityTypeId, setCreateEntityTypeId] = useState('')
 const [createEntityData, setCreateEntityData] = useState('{}')
 const [creatingEntity, setCreatingEntity] = useState(false)

 // Filters
 const [searchQuery, setSearchQuery] = useState('')
 const [filterType, setFilterType] = useState<string>('__all__')
 const [filterStatus, setFilterStatus] = useState<string>('__all__')

 // Delete confirmation state
 const [deleteTarget, setDeleteTarget] = useState<{ kind: 'entity' | 'type'; id: string; name: string } | null>(null)

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const [types, ents] = await Promise.all([
 client.listEntityTypes(orgSlug),
 client.listEntities(orgSlug),
 ])
 setEntityTypes(types.data)
 setEntities(ents.data)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load entities')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 // Map type IDs to names
 const typeMap = new Map(entityTypes.map((t) => [t.id, t]))

 // Unique statuses from loaded entities
 const uniqueStatuses = [...new Set(entities.map((e) => e.status))].sort()

 // Filtered entities
 const filteredEntities = entities.filter((e) => {
 if (filterType !== '__all__' && e.entity_type_id !== filterType) return false
 if (filterStatus !== '__all__' && e.status !== filterStatus) return false
 if (searchQuery) {
 const q = searchQuery.toLowerCase()
 const typeName = (e.entity_type_name ?? typeMap.get(e.entity_type_id)?.display_name ?? '').toLowerCase()
 if (
 !e.display_name.toLowerCase().includes(q) &&
 !typeName.includes(q) &&
 !e.status.toLowerCase().includes(q)
 ) return false
 }
 return true
 })

 const hasActiveFilters = searchQuery || filterType !== '__all__' || filterStatus !== '__all__'

 const clearFilters = () => {
 setSearchQuery('')
 setFilterType('__all__')
 setFilterStatus('__all__')
 }

 // --- Handlers ---

 const handleCreateEntityType = async () => {
 if (!token || !orgSlug) return
 if (!createTypeName.trim()) {
 toast.error('Name is required')
 return
 }
 let parsedSchema: unknown
 try {
 parsedSchema = JSON.parse(createTypeSchema)
 } catch {
 toast.error('Schema must be valid JSON')
 return
 }
 setCreatingType(true)
 try {
 const client = new BpeClient(token)
 await client.createEntityType({
 organization_id: orgSlug,
 name: createTypeName.trim(),
 schema: parsedSchema,
 })
 toast.success(`Entity type "${createTypeName.trim()}" created`)
 setShowCreateType(false)
 setCreateTypeName('')
 setCreateTypeSchema('{}')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to create entity type')
 } finally {
 setCreatingType(false)
 }
 }

 const handleCreateEntity = async () => {
 if (!token || !orgSlug) return
 if (!createEntityName.trim()) {
 toast.error('Name is required')
 return
 }
 if (!createEntityTypeId) {
 toast.error('Entity type is required')
 return
 }
 let parsedData: unknown
 try {
 parsedData = JSON.parse(createEntityData)
 } catch {
 toast.error('Data must be valid JSON')
 return
 }
 setCreatingEntity(true)
 try {
 const client = new BpeClient(token)
 await client.createEntity({
 organization_id: orgSlug,
 entity_type_id: createEntityTypeId,
 name: createEntityName.trim(),
 data: parsedData,
 })
 toast.success(`Entity "${createEntityName.trim()}" created`)
 setShowCreateEntity(false)
 setCreateEntityName('')
 setCreateEntityTypeId('')
 setCreateEntityData('{}')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to create entity')
 } finally {
 setCreatingEntity(false)
 }
 }

 const handleDelete = async () => {
 if (!token || !deleteTarget) return
 const client = new BpeClient(token)
 try {
 if (deleteTarget.kind === 'type') {
 await client.deleteEntityType(deleteTarget.id)
 toast.success(`Entity type "${deleteTarget.name}" deleted`)
 } else {
 await client.deleteEntity(deleteTarget.id)
 toast.success(`Entity "${deleteTarget.name}" deleted`)
 }
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Delete failed')
 }
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view entities.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Entities</h1>
 <Button variant="outline" size="sm" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Refresh</Button>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Tabs */}
 <div className="flex gap-2 border-b border-gray-200 pb-1">
 <button
 onClick={() => setTab('entities')}
 className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors ${
 tab === 'entities'
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 Entities ({entities.length})
 </button>
 <button
 onClick={() => setTab('types')}
 className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors ${
 tab === 'types'
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 Entity Types ({entityTypes.length})
 </button>
 </div>

 {tab === 'entities' && (
 <div className="space-y-3">
 {/* Filter bar */}
 <div className="flex flex-wrap items-center gap-2">
 <div className="relative flex-1 min-w-[200px]">
 <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
 <Input
 placeholder="Search entities..."
 value={searchQuery}
 onChange={(e) => setSearchQuery(e.target.value)}
 className="pl-8 h-9"
 />
 </div>
 <Select value={filterType} onValueChange={setFilterType}>
 <SelectTrigger className="w-[180px] h-9">
 <SelectValue placeholder="All types" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="__all__">All types</SelectItem>
 {entityTypes.map((t) => (
 <SelectItem key={t.id} value={t.id}>{t.display_name}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 <Select value={filterStatus} onValueChange={setFilterStatus}>
 <SelectTrigger className="w-[140px] h-9">
 <SelectValue placeholder="All statuses" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="__all__">All statuses</SelectItem>
 {uniqueStatuses.map((s) => (
 <SelectItem key={s} value={s}>{s.charAt(0).toUpperCase() + s.slice(1)}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 {hasActiveFilters && (
 <Button variant="ghost" size="sm" onClick={clearFilters} className="h-9 px-2 text-gray-500">
 <X className="w-4 h-4 mr-1" />Clear
 </Button>
 )}
 <Button size="sm" onClick={() => setShowCreateEntity(true)} className="h-9 ml-auto">
 <Plus className="w-4 h-4 mr-1" />Create Entity
 </Button>
 </div>

 {/* Results */}
 {entities.length === 0 ? (
 <div className="text-center py-12">
 <Database className="w-12 h-12 mx-auto text-gray-300 mb-3" />
 <p className="text-gray-500">No entities registered</p>
 </div>
 ) : filteredEntities.length === 0 ? (
 <div className="text-center py-8">
 <p className="text-gray-500 text-sm">No entities match your filters</p>
 <Button variant="link" size="sm" onClick={clearFilters} className="mt-1">Clear filters</Button>
 </div>
 ) : (
 <>
 {hasActiveFilters && (
 <p className="text-xs text-gray-400">
 Showing {filteredEntities.length} of {entities.length} entities
 </p>
 )}
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-500">Name</th>
 <th className="text-left py-2 px-3 font-medium text-gray-500">Type</th>
 <th className="text-left py-2 px-3 font-medium text-gray-500">Status</th>
 <th className="text-left py-2 px-3 font-medium text-gray-500">Created</th>
 <th className="text-right py-2 px-3 font-medium text-gray-500">Actions</th>
 </tr>
 </thead>
 <tbody>
 {filteredEntities.map((e) => (
 <tr key={e.id} className="border-b border-gray-100 hover:bg-gray-50">
 <td className="py-2 px-3 font-medium text-gray-900">{e.display_name}</td>
 <td className="py-2 px-3">
 <Badge variant="secondary">{e.entity_type_name ?? typeMap.get(e.entity_type_id)?.display_name ?? e.entity_type_id.slice(0, 8)}</Badge>
 </td>
 <td className="py-2 px-3">
 <Badge variant={e.status === 'active' ? 'default' : 'outline'}>
 {e.status}
 </Badge>
 </td>
 <td className="py-2 px-3 text-gray-400 text-xs">{new Date(e.created_at).toLocaleDateString()}</td>
 <td className="py-2 px-3 text-right">
 <Button
 variant="ghost"
 size="sm"
 className="text-red-500 hover:text-red-700 hover:bg-red-50"
 onClick={() => setDeleteTarget({ kind: 'entity', id: e.id, name: e.display_name })}
 >
 <Trash2 className="w-4 h-4" />
 </Button>
 </td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </>
 )}
 </div>
 )}

 {tab === 'types' && (
 <div className="space-y-4">
 <div className="flex justify-end">
 <Button size="sm" onClick={() => setShowCreateType(true)}>
 <Plus className="w-4 h-4 mr-2" />Create Entity Type
 </Button>
 </div>
 <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
 {entityTypes.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-8 col-span-full">No entity types defined</p>
 ) : entityTypes.map((t) => (
 <Card key={t.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center justify-between mb-2">
 <h3 className="font-semibold text-gray-900">{t.display_name}</h3>
 <div className="flex items-center gap-2">
 {t.is_system && <Badge variant="secondary">System</Badge>}
 <Button
 variant="ghost"
 size="sm"
 className="text-red-500 hover:text-red-700 hover:bg-red-50 h-7 w-7 p-0"
 onClick={() => setDeleteTarget({ kind: 'type', id: t.id, name: t.display_name })}
 >
 <Trash2 className="w-4 h-4" />
 </Button>
 </div>
 </div>
 <p className="text-xs text-gray-400 font-mono">{t.name}</p>
 {t.description && (
 <p className="text-xs text-gray-400 mt-1">{t.description}</p>
 )}
 {(t.core_fields.length > 0 || t.custom_fields.length > 0) && (
 <p className="text-xs text-gray-400 mt-0.5">{t.core_fields.length + t.custom_fields.length} fields</p>
 )}
 </CardContent>
 </Card>
 ))}
 </div>
 </div>
 )}

 {/* Create Entity Type Dialog */}
 <Dialog open={showCreateType} onOpenChange={setShowCreateType}>
 <DialogContent>
 <DialogHeader>
 <DialogTitle>Create Entity Type</DialogTitle>
 <DialogDescription>Define a new entity type for your organization.</DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="et-name">Name *</Label>
 <Input
 id="et-name"
 placeholder="e.g. Customer, Product"
 value={createTypeName}
 onChange={(e) => setCreateTypeName(e.target.value)}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="et-schema">Schema (JSON)</Label>
 <Textarea
 id="et-schema"
 rows={4}
 className="font-mono text-sm"
 value={createTypeSchema}
 onChange={(e) => setCreateTypeSchema(e.target.value)}
 />
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setShowCreateType(false)} disabled={creatingType}>Cancel</Button>
 <Button onClick={handleCreateEntityType} disabled={creatingType}>
 {creatingType && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
 Create
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Create Entity Dialog */}
 <Dialog open={showCreateEntity} onOpenChange={setShowCreateEntity}>
 <DialogContent>
 <DialogHeader>
 <DialogTitle>Create Entity</DialogTitle>
 <DialogDescription>Add a new entity instance.</DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-2">
 <Label htmlFor="e-type">Entity Type *</Label>
 <select
 id="e-type"
 className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
 value={createEntityTypeId}
 onChange={(e) => setCreateEntityTypeId(e.target.value)}
 >
 <option value="">Select a type...</option>
 {entityTypes.map((t) => (
 <option key={t.id} value={t.id}>{t.display_name}</option>
 ))}
 </select>
 </div>
 <div className="space-y-2">
 <Label htmlFor="e-name">Name *</Label>
 <Input
 id="e-name"
 placeholder="Entity name"
 value={createEntityName}
 onChange={(e) => setCreateEntityName(e.target.value)}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="e-data">Data (JSON)</Label>
 <Textarea
 id="e-data"
 rows={4}
 className="font-mono text-sm"
 value={createEntityData}
 onChange={(e) => setCreateEntityData(e.target.value)}
 />
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setShowCreateEntity(false)} disabled={creatingEntity}>Cancel</Button>
 <Button onClick={handleCreateEntity} disabled={creatingEntity}>
 {creatingEntity && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
 Create
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete Confirmation */}
 <ConfirmDialog
 open={!!deleteTarget}
 onOpenChange={(open) => { if (!open) setDeleteTarget(null) }}
 title={`Delete ${deleteTarget?.kind === 'type' ? 'Entity Type' : 'Entity'}`}
 description={`Are you sure you want to delete "${deleteTarget?.name}"? This action cannot be undone.`}
 confirmLabel="Delete"
 variant="danger"
 onConfirm={handleDelete}
 />
 </div>
 )
}
