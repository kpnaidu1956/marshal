import { useState, useEffect, useCallback, useMemo } from 'react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import {
 Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import {
 Loader2, RefreshCw, Plus, Trash2, Search, X, Pencil, Upload,
 Users, ArrowUpDown, Phone,
} from 'lucide-react'

// ── Types ──

interface Employee {
 id: string
 first_name: string
 last_name: string
 rank: string
 shift_assignment: string
 station?: string
 phone1?: string
 phone2?: string
 address_line1?: string
 city?: string
 state?: string
 zip?: string
 status: string
 organization_id: string
 created_at?: string
 updated_at?: string
}

type SortField = 'name' | 'rank' | 'shift_assignment' | 'station' | 'status'
type SortDir = 'asc' | 'desc'

// ── Constants ──

const RANKS = [
 'Administration',
 'Chief',
 'Division Chief',
 'Battalion Chief',
 'Captain',
 'Lieutenant',
 'Engineer',
 'Firefighter',
 'Reserve',
 'Technical Specialist',
 'Paramedic',
] as const

const SHIFTS = ['A', 'B', 'C'] as const

function rankBadgeClass(rank: string): string {
 const r = rank.toLowerCase()
 if (r === 'chief' || r === 'battalion chief' || r === 'captain' || r === 'lieutenant')
 return 'bg-red-100 text-red-800 border-red-200'
 if (r === 'firefighter' || r === 'engineer' || r === 'paramedic')
 return 'bg-blue-100 text-blue-800 border-blue-200'
 if (r === 'reserve' || r === 'technical specialist')
 return 'bg-green-100 text-green-800 border-green-200'
 if (r === 'administration')
 return 'bg-purple-100 text-purple-800 border-purple-200'
 return ''
}

const EMPTY_FORM = {
 first_name: '',
 last_name: '',
 rank: '',
 shift_assignment: '',
 phone1: '',
 phone2: '',
 address_line1: '',
 city: '',
 state: '',
 zip: '',
}

// ── Component ──

export function EmployeeRosterPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 // Data
 const [employees, setEmployees] = useState<Employee[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 // Filters
 const [searchQuery, setSearchQuery] = useState('')
 const [filterShift, setFilterShift] = useState<string>('__all__')
 const [filterRank, setFilterRank] = useState<string>('__all__')
 const [filterStatus, setFilterStatus] = useState<string>('active')

 // Sort
 const [sortField, setSortField] = useState<SortField>('name')
 const [sortDir, setSortDir] = useState<SortDir>('asc')

 // Create / Edit dialog
 const [showForm, setShowForm] = useState(false)
 const [editingId, setEditingId] = useState<string | null>(null)
 const [form, setForm] = useState(EMPTY_FORM)
 const [saving, setSaving] = useState(false)

 // Import dialog
 const [showImport, setShowImport] = useState(false)
 const [importJson, setImportJson] = useState('')
 const [importing, setImporting] = useState(false)

 // Delete
 const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null)

 // ── Fetch ──

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkListEmployees(orgSlug)
 setEmployees(res.data as Employee[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load employees')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 // ── Derived data ──

 const uniqueRanks = useMemo(
 () => [...new Set(employees.map((e) => e.rank).filter(Boolean))].sort(),
 [employees],
 )

 const uniqueStations = useMemo(
 () => [...new Set(employees.map((e) => e.station).filter(Boolean))].sort() as string[],
 [employees],
 )

 const filtered = useMemo(() => {
 let list = employees
 if (filterStatus !== '__all__') list = list.filter((e) => e.status === filterStatus)
 if (filterShift !== '__all__') list = list.filter((e) => e.shift_assignment === filterShift)
 if (filterRank !== '__all__') list = list.filter((e) => e.rank === filterRank)
 if (searchQuery) {
 const q = searchQuery.toLowerCase()
 list = list.filter((e) =>
 `${e.first_name} ${e.last_name}`.toLowerCase().includes(q) ||
 (e.rank ?? '').toLowerCase().includes(q) ||
 (e.station ?? '').toLowerCase().includes(q) ||
 (e.phone1 ?? '').includes(q),
 )
 }
 // Sort
 list = [...list].sort((a, b) => {
 let av: string, bv: string
 switch (sortField) {
 case 'name': av = `${a.last_name} ${a.first_name}`.toLowerCase(); bv = `${b.last_name} ${b.first_name}`.toLowerCase(); break
 case 'rank': av = (a.rank ?? '').toLowerCase(); bv = (b.rank ?? '').toLowerCase(); break
 case 'shift_assignment': av = a.shift_assignment ?? ''; bv = b.shift_assignment ?? ''; break
 case 'station': av = (a.station ?? '').toLowerCase(); bv = (b.station ?? '').toLowerCase(); break
 case 'status': av = a.status; bv = b.status; break
 default: av = ''; bv = ''
 }
 const cmp = av.localeCompare(bv)
 return sortDir === 'asc' ? cmp : -cmp
 })
 return list
 }, [employees, filterStatus, filterShift, filterRank, searchQuery, sortField, sortDir])

 const hasActiveFilters = searchQuery || filterShift !== '__all__' || filterRank !== '__all__' || filterStatus !== 'active'

 const clearFilters = () => {
 setSearchQuery('')
 setFilterShift('__all__')
 setFilterRank('__all__')
 setFilterStatus('active')
 }

 const toggleSort = (field: SortField) => {
 if (sortField === field) {
 setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'))
 } else {
 setSortField(field)
 setSortDir('asc')
 }
 }

 // ── Handlers ──

 const openCreate = () => {
 setEditingId(null)
 setForm(EMPTY_FORM)
 setShowForm(true)
 }

 const openEdit = (emp: Employee) => {
 setEditingId(emp.id)
 setForm({
 first_name: emp.first_name,
 last_name: emp.last_name,
 rank: emp.rank ?? '',
 shift_assignment: emp.shift_assignment ?? '',
 phone1: emp.phone1 ?? '',
 phone2: emp.phone2 ?? '',
 address_line1: emp.address_line1 ?? '',
 city: emp.city ?? '',
 state: emp.state ?? '',
 zip: emp.zip ?? '',
 })
 setShowForm(true)
 }

 const handleSave = async () => {
 if (!token || !orgSlug) return
 if (!form.first_name.trim() || !form.last_name.trim()) {
 toast.error('First and last name are required')
 return
 }
 if (!form.rank) {
 toast.error('Rank is required')
 return
 }
 if (!form.shift_assignment) {
 toast.error('Shift assignment is required')
 return
 }
 setSaving(true)
 try {
 const client = new BpeClient(token)
 const body = { ...form, organization_id: orgSlug }
 if (editingId) {
 await client.tkUpdateEmployee(editingId, body)
 toast.success('Employee updated')
 } else {
 await client.tkCreateEmployee(body)
 toast.success('Employee created')
 }
 setShowForm(false)
 setForm(EMPTY_FORM)
 setEditingId(null)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to save employee')
 } finally {
 setSaving(false)
 }
 }

 const handleDelete = async () => {
 if (!token || !deleteTarget) return
 try {
 const client = new BpeClient(token)
 await client.tkDeleteEmployee(deleteTarget.id)
 toast.success(`${deleteTarget.name} deactivated`)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Delete failed')
 }
 }

 const handleImport = async () => {
 if (!token || !orgSlug) return
 let parsed: unknown[]
 try {
 parsed = JSON.parse(importJson)
 if (!Array.isArray(parsed)) throw new Error('Must be a JSON array')
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Invalid JSON array')
 return
 }
 setImporting(true)
 try {
 const client = new BpeClient(token)
 const res = await client.tkImportEmployees({
 organization_id: orgSlug,
 employees: parsed,
 })
 toast.success(`Imported: ${res.created} created, ${res.skipped} skipped`)
 if (res.errors?.length) {
 toast.warning(`${res.errors.length} error(s) during import`)
 }
 setShowImport(false)
 setImportJson('')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Import failed')
 } finally {
 setImporting(false)
 }
 }

 // ── Render ──

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view employees.</p>
 </div>
 )
 }

 if (loading) {
 return (
 <div className="flex items-center justify-center h-64">
 <Loader2 className="w-6 h-6 animate-spin text-indigo-500" />
 </div>
 )
 }

 const SortHeader = ({ field, label }: { field: SortField; label: string }) => (
 <th
 className="text-left py-2 px-3 font-medium text-gray-500 cursor-pointer select-none hover:text-gray-700"
 onClick={() => toggleSort(field)}
 >
 <span className="inline-flex items-center gap-1">
 {label}
 <ArrowUpDown className={`w-3 h-3 ${sortField === field ? 'text-indigo-500' : 'text-gray-300'}`} />
 </span>
 </th>
 )

 return (
 <div className="space-y-6">
 {/* Header */}
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Employee Roster</h1>
 <div className="flex items-center gap-2">
 <Button variant="outline" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4 mr-2" />Refresh
 </Button>
 </div>
 </div>

 {error && (
 <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>
 )}

 {/* Summary cards */}
 <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
 <Card>
 <CardHeader className="pb-2 pt-4 px-4">
 <CardTitle className="text-xs font-medium text-gray-500 uppercase tracking-wide">Total</CardTitle>
 </CardHeader>
 <CardContent className="px-4 pb-4">
 <p className="text-2xl font-bold">{employees.filter((e) => e.status === 'active').length}</p>
 </CardContent>
 </Card>
 {SHIFTS.map((s) => (
 <Card key={s}>
 <CardHeader className="pb-2 pt-4 px-4">
 <CardTitle className="text-xs font-medium text-gray-500 uppercase tracking-wide">Shift {s}</CardTitle>
 </CardHeader>
 <CardContent className="px-4 pb-4">
 <p className="text-2xl font-bold">
 {employees.filter((e) => e.shift_assignment === s && e.status === 'active').length}
 </p>
 </CardContent>
 </Card>
 ))}
 </div>

 {/* Filter bar */}
 <div className="flex flex-wrap items-center gap-2">
 <div className="relative flex-1 min-w-[200px]">
 <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
 <Input
 placeholder="Search by name, rank, station, phone..."
 value={searchQuery}
 onChange={(e) => setSearchQuery(e.target.value)}
 className="pl-8 h-9"
 />
 </div>
 <Select value={filterShift} onValueChange={setFilterShift}>
 <SelectTrigger className="w-[120px] h-9">
 <SelectValue placeholder="All shifts" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="__all__">All shifts</SelectItem>
 {SHIFTS.map((s) => (
 <SelectItem key={s} value={s}>Shift {s}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 <Select value={filterRank} onValueChange={setFilterRank}>
 <SelectTrigger className="w-[160px] h-9">
 <SelectValue placeholder="All ranks" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="__all__">All ranks</SelectItem>
 {(uniqueRanks.length > 0 ? uniqueRanks : RANKS as unknown as string[]).map((r) => (
 <SelectItem key={r} value={r}>{r}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 <Select value={filterStatus} onValueChange={setFilterStatus}>
 <SelectTrigger className="w-[130px] h-9">
 <SelectValue placeholder="Status" />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="__all__">All statuses</SelectItem>
 <SelectItem value="active">Active</SelectItem>
 <SelectItem value="inactive">Inactive</SelectItem>
 </SelectContent>
 </Select>
 {hasActiveFilters && (
 <Button variant="ghost" size="sm" onClick={clearFilters} className="h-9 px-2 text-gray-500">
 <X className="w-4 h-4 mr-1" />Clear
 </Button>
 )}
 <div className="flex items-center gap-2 ml-auto">
 <Button variant="outline" size="sm" onClick={() => setShowImport(true)} className="h-9">
 <Upload className="w-4 h-4 mr-1" />Import
 </Button>
 <Button size="sm" onClick={openCreate} className="h-9">
 <Plus className="w-4 h-4 mr-1" />Add Employee
 </Button>
 </div>
 </div>

 {/* Results */}
 {employees.length === 0 ? (
 <div className="text-center py-12">
 <Users className="w-12 h-12 mx-auto text-gray-300 mb-3" />
 <p className="text-gray-500">No employees found</p>
 <Button size="sm" className="mt-3" onClick={openCreate}>
 <Plus className="w-4 h-4 mr-1" />Add First Employee
 </Button>
 </div>
 ) : filtered.length === 0 ? (
 <div className="text-center py-8">
 <p className="text-gray-500 text-sm">No employees match your filters</p>
 <Button variant="link" size="sm" onClick={clearFilters} className="mt-1">Clear filters</Button>
 </div>
 ) : (
 <>
 {hasActiveFilters && (
 <p className="text-xs text-gray-400">
 Showing {filtered.length} of {employees.length} employees
 </p>
 )}
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <SortHeader field="name" label="Name" />
 <SortHeader field="rank" label="Rank" />
 <SortHeader field="shift_assignment" label="Shift" />
 <SortHeader field="station" label="Station" />
 <th className="text-left py-2 px-3 font-medium text-gray-500">Phone</th>
 <SortHeader field="status" label="Status" />
 <th className="text-right py-2 px-3 font-medium text-gray-500">Actions</th>
 </tr>
 </thead>
 <tbody>
 {filtered.map((emp) => (
 <tr
 key={emp.id}
 className="border-b border-gray-100 hover:bg-gray-50 cursor-pointer"
 onClick={() => openEdit(emp)}
 >
 <td className="py-2 px-3 font-medium text-gray-900">
 {emp.last_name}, {emp.first_name}
 </td>
 <td className="py-2 px-3">
 <Badge variant="outline" className={rankBadgeClass(emp.rank)}>
 {emp.rank}
 </Badge>
 </td>
 <td className="py-2 px-3">
 <Badge variant="secondary">{emp.shift_assignment}</Badge>
 </td>
 <td className="py-2 px-3 text-gray-600">
 {emp.station ?? '-'}
 </td>
 <td className="py-2 px-3 text-gray-600">
 {emp.phone1 ? (
 <span className="inline-flex items-center gap-1">
 <Phone className="w-3 h-3" />{emp.phone1}
 </span>
 ) : '-'}
 </td>
 <td className="py-2 px-3">
 <Badge variant={emp.status === 'active' ? 'default' : 'outline'}>
 {emp.status}
 </Badge>
 </td>
 <td className="py-2 px-3 text-right">
 <div className="inline-flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
 <Button
 variant="ghost"
 size="sm"
 className="h-7 w-7 p-0"
 onClick={() => openEdit(emp)}
 >
 <Pencil className="w-4 h-4" />
 </Button>
 <Button
 variant="ghost"
 size="sm"
 className="text-red-500 hover:text-red-700 hover:bg-red-50 h-7 w-7 p-0"
 onClick={() => setDeleteTarget({ id: emp.id, name: `${emp.first_name} ${emp.last_name}` })}
 >
 <Trash2 className="w-4 h-4" />
 </Button>
 </div>
 </td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </>
 )}

 {/* Create / Edit Employee Dialog */}
 <Dialog open={showForm} onOpenChange={setShowForm}>
 <DialogContent className="max-w-lg">
 <DialogHeader>
 <DialogTitle>{editingId ? 'Edit Employee' : 'Add Employee'}</DialogTitle>
 <DialogDescription>
 {editingId ? 'Update employee information.' : 'Add a new employee to the roster.'}
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-2">
 <Label htmlFor="emp-first">First Name *</Label>
 <Input
 id="emp-first"
 value={form.first_name}
 onChange={(e) => setForm((f) => ({ ...f, first_name: e.target.value }))}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="emp-last">Last Name *</Label>
 <Input
 id="emp-last"
 value={form.last_name}
 onChange={(e) => setForm((f) => ({ ...f, last_name: e.target.value }))}
 />
 </div>
 </div>
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-2">
 <Label htmlFor="emp-rank">Rank *</Label>
 <Select value={form.rank} onValueChange={(v) => setForm((f) => ({ ...f, rank: v }))}>
 <SelectTrigger id="emp-rank" className="h-10">
 <SelectValue placeholder="Select rank..." />
 </SelectTrigger>
 <SelectContent>
 {RANKS.map((r) => (
 <SelectItem key={r} value={r}>{r}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 <div className="space-y-2">
 <Label htmlFor="emp-shift">Shift *</Label>
 <Select value={form.shift_assignment} onValueChange={(v) => setForm((f) => ({ ...f, shift_assignment: v }))}>
 <SelectTrigger id="emp-shift" className="h-10">
 <SelectValue placeholder="Select shift..." />
 </SelectTrigger>
 <SelectContent>
 {SHIFTS.map((s) => (
 <SelectItem key={s} value={s}>Shift {s}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 </div>
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-2">
 <Label htmlFor="emp-phone1">Phone 1</Label>
 <Input
 id="emp-phone1"
 value={form.phone1}
 onChange={(e) => setForm((f) => ({ ...f, phone1: e.target.value }))}
 placeholder="(555) 123-4567"
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="emp-phone2">Phone 2</Label>
 <Input
 id="emp-phone2"
 value={form.phone2}
 onChange={(e) => setForm((f) => ({ ...f, phone2: e.target.value }))}
 />
 </div>
 </div>
 <div className="space-y-2">
 <Label htmlFor="emp-addr">Address</Label>
 <Input
 id="emp-addr"
 value={form.address_line1}
 onChange={(e) => setForm((f) => ({ ...f, address_line1: e.target.value }))}
 placeholder="123 Main St"
 />
 </div>
 <div className="grid grid-cols-3 gap-4">
 <div className="space-y-2">
 <Label htmlFor="emp-city">City</Label>
 <Input
 id="emp-city"
 value={form.city}
 onChange={(e) => setForm((f) => ({ ...f, city: e.target.value }))}
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="emp-state">State</Label>
 <Input
 id="emp-state"
 value={form.state}
 onChange={(e) => setForm((f) => ({ ...f, state: e.target.value }))}
 maxLength={2}
 placeholder="CA"
 />
 </div>
 <div className="space-y-2">
 <Label htmlFor="emp-zip">ZIP</Label>
 <Input
 id="emp-zip"
 value={form.zip}
 onChange={(e) => setForm((f) => ({ ...f, zip: e.target.value }))}
 maxLength={10}
 placeholder="92028"
 />
 </div>
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setShowForm(false)} disabled={saving}>Cancel</Button>
 <Button onClick={handleSave} disabled={saving}>
 {saving && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
 {editingId ? 'Save Changes' : 'Add Employee'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Import Dialog */}
 <Dialog open={showImport} onOpenChange={setShowImport}>
 <DialogContent className="max-w-lg">
 <DialogHeader>
 <DialogTitle>Bulk Import Employees</DialogTitle>
 <DialogDescription>
 Paste a JSON array of employee objects. Each object should include first_name, last_name, rank, and shift_assignment.
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <Textarea
 rows={10}
 className="font-mono text-sm"
 placeholder={'[\n { "first_name": "John", "last_name": "Doe", "rank": "Firefighter", "shift_assignment": "A" }\n]'}
 value={importJson}
 onChange={(e) => setImportJson(e.target.value)}
 />
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setShowImport(false)} disabled={importing}>Cancel</Button>
 <Button onClick={handleImport} disabled={importing || !importJson.trim()}>
 {importing && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
 Import
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete Confirmation */}
 <ConfirmDialog
 open={!!deleteTarget}
 onOpenChange={(open) => { if (!open) setDeleteTarget(null) }}
 title="Deactivate Employee"
 description={`Are you sure you want to deactivate "${deleteTarget?.name}"? They can be reactivated later.`}
 confirmLabel="Deactivate"
 variant="danger"
 onConfirm={handleDelete}
 />
 </div>
 )
}
