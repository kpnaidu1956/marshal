import { useState, useEffect, useCallback } from 'react'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
 Dialog,
 DialogContent,
 DialogHeader,
 DialogTitle,
 DialogFooter,
 DialogDescription,
} from '@/components/ui/dialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import {
 Loader2,
 RefreshCw,
 Settings,
 MapPin,
 Tag,
 Calendar,
 Plus,
 Pencil,
 Lock,
 Save,
} from 'lucide-react'

type TabKey = 'kelly' | 'stations' | 'paycodes' | 'periods'

// --- Kelly Config ---

interface KellyConfig {
 epoch_date: string
 cycle_length: number
 shift_labels: string[]
 rotation_pattern: string[]
}

const EMPTY_KELLY: KellyConfig = {
 epoch_date: '',
 cycle_length: 21,
 shift_labels: ['A', 'B', 'C'],
 rotation_pattern: [],
}

// --- Station ---

interface Station {
 id: string
 name: string
 station_number: number
 address: string | null
 min_staffing: number
 is_active: boolean
}

// --- Pay Code ---

interface PayCode {
 id: string
 code: string
 display_name: string
 category: string
 multiplier: number
 is_overtime: boolean
}

// --- Period ---

interface TimecardPeriod {
 id: string
 period_start: string
 period_end: string
 status: string
}

export function TimekeepingSettingsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [tab, setTab] = useState<TabKey>('kelly')
 const [loading, setLoading] = useState(false)
 const [error, setError] = useState<string | null>(null)

 // Kelly
 const [kellyConfig, setKellyConfig] = useState<KellyConfig>({ ...EMPTY_KELLY })
 const [kellySaving, setKellySaving] = useState(false)

 // Stations
 const [stations, setStations] = useState<Station[]>([])
 const [stationDialogOpen, setStationDialogOpen] = useState(false)
 const [editingStation, setEditingStation] = useState<Station | null>(null)
 const [stationForm, setStationForm] = useState({ name: '', station_number: 1, address: '', min_staffing: 3 })
 const [stationSaving, setStationSaving] = useState(false)

 // Pay Codes
 const [payCodes, setPayCodes] = useState<PayCode[]>([])
 const [payCodeDialogOpen, setPayCodeDialogOpen] = useState(false)
 const [editingPayCode, setEditingPayCode] = useState<PayCode | null>(null)
 const [payCodeForm, setPayCodeForm] = useState({
 code: '',
 display_name: '',
 category: 'work',
 multiplier: 1.0,
 is_overtime: false,
 })
 const [payCodeSaving, setPayCodeSaving] = useState(false)

 // Periods
 const [periods, setPeriods] = useState<TimecardPeriod[]>([])
 const [periodDialogOpen, setPeriodDialogOpen] = useState(false)
 const [periodForm, setPeriodForm] = useState({ start_date: '', end_date: '' })
 const [periodSaving, setPeriodSaving] = useState(false)
 const [closingPeriod, setClosingPeriod] = useState<string | null>(null)

 // --- Fetch functions ---

 const fetchKelly = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkGetKellyConfig(orgSlug)
 if (res.data) {
 const d = res.data as KellyConfig
 setKellyConfig({
 epoch_date: d.epoch_date || '',
 cycle_length: d.cycle_length || 21,
 shift_labels: d.shift_labels || ['A', 'B', 'C'],
 rotation_pattern: d.rotation_pattern || [],
 })
 }
 } catch {
 // No config yet
 setKellyConfig({ ...EMPTY_KELLY })
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 const fetchStations = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkListStations(orgSlug)
 setStations(res.data as Station[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load stations')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 const fetchPayCodes = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkListPayCodes(orgSlug)
 setPayCodes(res.data as PayCode[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load pay codes')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 const fetchPeriods = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.tkListPeriods(orgSlug)
 setPeriods(res.data as TimecardPeriod[])
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load periods')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => {
 switch (tab) {
 case 'kelly':
 fetchKelly()
 break
 case 'stations':
 fetchStations()
 break
 case 'paycodes':
 fetchPayCodes()
 break
 case 'periods':
 fetchPeriods()
 break
 }
 }, [tab, fetchKelly, fetchStations, fetchPayCodes, fetchPeriods])

 // --- Kelly Save ---

 const saveKelly = async () => {
 if (!token || !orgSlug) return
 setKellySaving(true)
 try {
 const client = new BpeClient(token)
 await client.tkUpsertKellyConfig({
 organization_id: orgSlug,
 epoch_date: kellyConfig.epoch_date,
 cycle_length: kellyConfig.cycle_length,
 shift_labels: kellyConfig.shift_labels,
 rotation_pattern: kellyConfig.rotation_pattern,
 })
 toast.success('Kelly schedule config saved')
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to save config')
 } finally {
 setKellySaving(false)
 }
 }

 // --- Station CRUD ---

 const openCreateStation = () => {
 setEditingStation(null)
 setStationForm({ name: '', code: '', address: '', min_staffing: 4 })
 setStationDialogOpen(true)
 }

 const openEditStation = (s: Station) => {
 setEditingStation(s)
 setStationForm({
 name: s.name,
 station_number: s.station_number,
 address: s.address || '',
 min_staffing: s.min_staffing,
 })
 setStationDialogOpen(true)
 }

 const saveStation = async () => {
 if (!token || !orgSlug) return
 setStationSaving(true)
 try {
 const client = new BpeClient(token)
 const body = {
 organization_id: orgSlug,
 name: stationForm.name.trim(),
 station_number: stationForm.station_number,
 address: stationForm.address.trim() || null,
 min_staffing: stationForm.min_staffing,
 }
 if (editingStation) {
 await client.tkUpdateStation(editingStation.id, body)
 toast.success('Station updated')
 } else {
 await client.tkCreateStation(body)
 toast.success('Station created')
 }
 setStationDialogOpen(false)
 await fetchStations()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to save station')
 } finally {
 setStationSaving(false)
 }
 }

 // --- Pay Code CRUD ---

 const openCreatePayCode = () => {
 setEditingPayCode(null)
 setPayCodeForm({ code: '', display_name: '', category: 'work', multiplier: 1.0, is_overtime: false })
 setPayCodeDialogOpen(true)
 }

 const openEditPayCode = (pc: PayCode) => {
 setEditingPayCode(pc)
 setPayCodeForm({
 code: pc.code,
 display_name: pc.display_name,
 category: pc.category,
 multiplier: pc.multiplier,
 is_overtime: pc.is_overtime,
 })
 setPayCodeDialogOpen(true)
 }

 const savePayCode = async () => {
 if (!token || !orgSlug) return
 setPayCodeSaving(true)
 try {
 const client = new BpeClient(token)
 const body = {
 organization_id: orgSlug,
 code: payCodeForm.code.trim(),
 display_name: payCodeForm.display_name.trim(),
 category: payCodeForm.category,
 multiplier: payCodeForm.multiplier,
 is_overtime: payCodeForm.is_overtime,
 }
 if (editingPayCode) {
 await client.tkUpdatePayCode(editingPayCode.id, body)
 toast.success('Pay code updated')
 } else {
 await client.tkCreatePayCode(body)
 toast.success('Pay code created')
 }
 setPayCodeDialogOpen(false)
 await fetchPayCodes()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to save pay code')
 } finally {
 setPayCodeSaving(false)
 }
 }

 // --- Period CRUD ---

 const openCreatePeriod = () => {
 setPeriodForm({ start_date: '', end_date: '' })
 setPeriodDialogOpen(true)
 }

 const savePeriod = async () => {
 if (!token || !orgSlug) return
 setPeriodSaving(true)
 try {
 const client = new BpeClient(token)
 await client.tkCreatePeriod({
 organization_id: orgSlug,
 period_start: periodForm.start_date,
 period_end: periodForm.end_date,
 })
 toast.success('Period created')
 setPeriodDialogOpen(false)
 await fetchPeriods()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to create period')
 } finally {
 setPeriodSaving(false)
 }
 }

 const closePeriod = async (id: string) => {
 if (!token) return
 setClosingPeriod(id)
 try {
 const client = new BpeClient(token)
 await client.tkClosePeriod(id)
 toast.success('Period closed')
 await fetchPeriods()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to close period')
 } finally {
 setClosingPeriod(null)
 }
 }

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to manage timekeeping settings.</p>
 </div>
 )
 }

 const TABS: { key: TabKey; label: string; icon: React.ReactNode }[] = [
 { key: 'kelly', label: 'Kelly Schedule', icon: <Calendar className="w-4 h-4" /> },
 { key: 'stations', label: 'Stations', icon: <MapPin className="w-4 h-4" /> },
 { key: 'paycodes', label: 'Pay Codes', icon: <Tag className="w-4 h-4" /> },
 { key: 'periods', label: 'Periods', icon: <Settings className="w-4 h-4" /> },
 ]

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Timekeeping Settings</h1>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Tabs */}
 <div className="flex gap-2 border-b border-gray-200 pb-1 overflow-x-auto">
 {TABS.map((t) => (
 <button
 key={t.key}
 onClick={() => setTab(t.key)}
 className={`flex items-center gap-1.5 px-4 py-2 text-sm font-medium rounded-t-lg transition-colors whitespace-nowrap ${
 tab === t.key
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 {t.icon}
 {t.label}
 </button>
 ))}
 </div>

 {loading && (
 <div className="flex items-center justify-center h-32"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 )}

 {/* Kelly Schedule Config */}
 {tab === 'kelly' && !loading && (
 <Card>
 <CardHeader>
 <CardTitle className="text-lg flex items-center gap-2">
 <Calendar className="w-5 h-5" />
 Kelly Schedule Configuration
 </CardTitle>
 </CardHeader>
 <CardContent className="space-y-4">
 <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
 <div className="space-y-1.5">
 <Label htmlFor="kelly-epoch">Epoch Date</Label>
 <Input
 id="kelly-epoch"
 type="date"
 value={kellyConfig.epoch_date}
 onChange={(e) => setKellyConfig((c) => ({ ...c, epoch_date: e.target.value }))}
 />
 <p className="text-xs text-gray-500">The reference start date for the rotation cycle.</p>
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="kelly-cycle">Cycle Length (days)</Label>
 <Input
 id="kelly-cycle"
 type="number"
 min={1}
 value={kellyConfig.cycle_length}
 onChange={(e) => setKellyConfig((c) => ({ ...c, cycle_length: parseInt(e.target.value, 10) || 21 }))}
 />
 </div>
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="kelly-shifts">Shift Labels (comma-separated)</Label>
 <Input
 id="kelly-shifts"
 value={kellyConfig.shift_labels.join(', ')}
 onChange={(e) =>
 setKellyConfig((c) => ({
 ...c,
 shift_labels: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
 }))
 }
 placeholder="A, B, C"
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="kelly-pattern">Rotation Pattern (comma-separated)</Label>
 <Input
 id="kelly-pattern"
 value={kellyConfig.rotation_pattern.join(', ')}
 onChange={(e) =>
 setKellyConfig((c) => ({
 ...c,
 rotation_pattern: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
 }))
 }
 placeholder="on, on, off, on, off, off, off"
 />
 <p className="text-xs text-gray-500">Pattern of on/off days within a single cycle for one shift.</p>
 </div>
 <div className="flex justify-end">
 <Button onClick={saveKelly} disabled={kellySaving || !kellyConfig.epoch_date}>
 {kellySaving ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Save className="w-4 h-4 mr-2" />}
 Save Configuration
 </Button>
 </div>
 </CardContent>
 </Card>
 )}

 {/* Stations */}
 {tab === 'stations' && !loading && (
 <div className="space-y-4">
 <div className="flex justify-between items-center">
 <p className="text-sm text-gray-500">{stations.length} station(s)</p>
 <div className="flex gap-2">
 <Button variant="outline" size="sm" onClick={fetchStations}><RefreshCw className="w-4 h-4 mr-1" />Refresh</Button>
 <Button size="sm" onClick={openCreateStation}><Plus className="w-4 h-4 mr-1" />Add Station</Button>
 </div>
 </div>

 {stations.length === 0 ? (
 <div className="text-center py-12">
 <MapPin className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No stations configured</p>
 </div>
 ) : (
 <div className="grid gap-3 md:grid-cols-2">
 {stations.map((s) => (
 <Card key={s.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-start justify-between">
 <div>
 <div className="flex items-center gap-2 mb-1">
 <h3 className="font-semibold text-gray-900">{s.name}</h3>
 <Badge variant="outline">{s.station_number}</Badge>
 {!s.is_active && <Badge variant="secondary">Inactive</Badge>}
 </div>
 {s.address && <p className="text-sm text-gray-500">{s.address}</p>}
 <p className="text-xs text-gray-400 mt-1">Min staffing: {s.min_staffing}</p>
 </div>
 <Button variant="ghost" size="sm" onClick={() => openEditStation(s)}>
 <Pencil className="w-4 h-4" />
 </Button>
 </div>
 </CardContent>
 </Card>
 ))}
 </div>
 )}
 </div>
 )}

 {/* Pay Codes */}
 {tab === 'paycodes' && !loading && (
 <div className="space-y-4">
 <div className="flex justify-between items-center">
 <p className="text-sm text-gray-500">{payCodes.length} pay code(s)</p>
 <div className="flex gap-2">
 <Button variant="outline" size="sm" onClick={fetchPayCodes}><RefreshCw className="w-4 h-4 mr-1" />Refresh</Button>
 <Button size="sm" onClick={openCreatePayCode}><Plus className="w-4 h-4 mr-1" />Add Pay Code</Button>
 </div>
 </div>

 {payCodes.length === 0 ? (
 <div className="text-center py-12">
 <Tag className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No pay codes configured</p>
 </div>
 ) : (
 <Card>
 <CardContent className="pt-4">
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-700">Code</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Display Name</th>
 <th className="text-left py-2 px-3 font-medium text-gray-700">Category</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Multiplier</th>
 <th className="text-center py-2 px-3 font-medium text-gray-700">OT</th>
 <th className="text-right py-2 px-3 font-medium text-gray-700">Action</th>
 </tr>
 </thead>
 <tbody>
 {payCodes.map((pc) => (
 <tr key={pc.id} className="border-b border-gray-100">
 <td className="py-2 px-3 font-mono text-gray-900">{pc.code}</td>
 <td className="py-2 px-3 text-gray-900">{pc.display_name}</td>
 <td className="py-2 px-3">
 <Badge variant="outline">{pc.category}</Badge>
 </td>
 <td className="py-2 px-3 text-right text-gray-600">{pc.multiplier}x</td>
 <td className="py-2 px-3 text-center">
 {pc.is_overtime ? (
 <Badge className="bg-amber-100 text-amber-700 text-xs">OT</Badge>
 ) : (
 <span className="text-gray-400">--</span>
 )}
 </td>
 <td className="py-2 px-3 text-right">
 <Button variant="ghost" size="sm" onClick={() => openEditPayCode(pc)}>
 <Pencil className="w-3.5 h-3.5" />
 </Button>
 </td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )}

 {/* Periods */}
 {tab === 'periods' && !loading && (
 <div className="space-y-4">
 <div className="flex justify-between items-center">
 <p className="text-sm text-gray-500">{periods.length} period(s)</p>
 <div className="flex gap-2">
 <Button variant="outline" size="sm" onClick={fetchPeriods}><RefreshCw className="w-4 h-4 mr-1" />Refresh</Button>
 <Button size="sm" onClick={openCreatePeriod}><Plus className="w-4 h-4 mr-1" />New Period</Button>
 </div>
 </div>

 {periods.length === 0 ? (
 <div className="text-center py-12">
 <Calendar className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No timecard periods</p>
 </div>
 ) : (
 <div className="space-y-3">
 {periods.map((p) => (
 <Card key={p.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-center justify-between">
 <div>
 <div className="flex items-center gap-2 mb-1">
 <h3 className="font-semibold text-gray-900">
 {p.period_start} &mdash; {p.period_end}
 </h3>
 <Badge
 className={
 p.status === 'open'
 ? 'bg-emerald-100 text-emerald-700'
 : p.status === 'closed'
 ? 'bg-gray-100 text-gray-700'
 : 'bg-amber-100 text-amber-700'
 }
 >
 {p.status}
 </Badge>
 </div>
 <p className="text-xs text-gray-500">{p.period_start} to {p.period_end}</p>
 </div>
 {p.status === 'open' && (
 <Button
 size="sm"
 variant="outline"
 onClick={() => closePeriod(p.id)}
 disabled={closingPeriod === p.id}
 >
 {closingPeriod === p.id ? (
 <Loader2 className="w-4 h-4 mr-1 animate-spin" />
 ) : (
 <Lock className="w-4 h-4 mr-1" />
 )}
 Close Period
 </Button>
 )}
 </div>
 </CardContent>
 </Card>
 ))}
 </div>
 )}
 </div>
 )}

 {/* Station Dialog */}
 <Dialog open={stationDialogOpen} onOpenChange={setStationDialogOpen}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>{editingStation ? 'Edit Station' : 'Add Station'}</DialogTitle>
 <DialogDescription>
 {editingStation ? 'Update station details.' : 'Create a new fire station.'}
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-1.5">
 <Label htmlFor="station-name">Station Name</Label>
 <Input
 id="station-name"
 value={stationForm.name}
 onChange={(e) => setStationForm((f) => ({ ...f, name: e.target.value }))}
 placeholder="Station 1"
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="station-number">Station Number</Label>
 <Input
 id="station-number"
 type="number"
 min={1}
 value={stationForm.station_number}
 onChange={(e) => setStationForm((f) => ({ ...f, station_number: parseInt(e.target.value) || 1 }))}
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="station-address">Address (optional)</Label>
 <Input
 id="station-address"
 value={stationForm.address}
 onChange={(e) => setStationForm((f) => ({ ...f, address: e.target.value }))}
 placeholder="123 Main St"
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="station-min">Min Staffing</Label>
 <Input
 id="station-min"
 type="number"
 min={0}
 value={stationForm.min_staffing}
 onChange={(e) => setStationForm((f) => ({ ...f, min_staffing: parseInt(e.target.value, 10) || 0 }))}
 />
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setStationDialogOpen(false)} disabled={stationSaving}>Cancel</Button>
 <Button onClick={saveStation} disabled={stationSaving || !stationForm.name.trim()}>
 {stationSaving ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : null}
 {editingStation ? 'Save' : 'Create'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Pay Code Dialog */}
 <Dialog open={payCodeDialogOpen} onOpenChange={setPayCodeDialogOpen}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>{editingPayCode ? 'Edit Pay Code' : 'Add Pay Code'}</DialogTitle>
 <DialogDescription>
 {editingPayCode ? 'Update pay code details.' : 'Create a new pay code.'}
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-1.5">
 <Label htmlFor="pc-code">Code</Label>
 <Input
 id="pc-code"
 value={payCodeForm.code}
 onChange={(e) => setPayCodeForm((f) => ({ ...f, code: e.target.value }))}
 placeholder="REG"
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="pc-name">Display Name</Label>
 <Input
 id="pc-name"
 value={payCodeForm.display_name}
 onChange={(e) => setPayCodeForm((f) => ({ ...f, display_name: e.target.value }))}
 placeholder="Regular"
 />
 </div>
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="pc-category">Category</Label>
 <select
 id="pc-category"
 value={payCodeForm.category}
 onChange={(e) => setPayCodeForm((f) => ({ ...f, category: e.target.value }))}
 className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
 >
 <option value="work">Work</option>
 <option value="leave">Leave</option>
 </select>
 </div>
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-1.5">
 <Label htmlFor="pc-mult">Multiplier</Label>
 <Input
 id="pc-mult"
 type="number"
 step="0.1"
 min={0}
 value={payCodeForm.multiplier}
 onChange={(e) => setPayCodeForm((f) => ({ ...f, multiplier: parseFloat(e.target.value) || 1 }))}
 />
 </div>
 <div className="flex items-end pb-1">
 <div className="flex items-center gap-2">
 <input
 id="pc-ot"
 type="checkbox"
 checked={payCodeForm.is_overtime}
 onChange={(e) => setPayCodeForm((f) => ({ ...f, is_overtime: e.target.checked }))}
 className="rounded border-gray-300"
 />
 <Label htmlFor="pc-ot" className="text-sm font-normal">Overtime</Label>
 </div>
 </div>
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setPayCodeDialogOpen(false)} disabled={payCodeSaving}>Cancel</Button>
 <Button onClick={savePayCode} disabled={payCodeSaving || !payCodeForm.code.trim() || !payCodeForm.display_name.trim()}>
 {payCodeSaving ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : null}
 {editingPayCode ? 'Save' : 'Create'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Period Dialog */}
 <Dialog open={periodDialogOpen} onOpenChange={setPeriodDialogOpen}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>Create Timecard Period</DialogTitle>
 <DialogDescription>Define the start and end dates for a new timecard period.</DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-1.5">
 <Label htmlFor="period-start">Start Date</Label>
 <Input
 id="period-start"
 type="date"
 value={periodForm.start_date}
 onChange={(e) => setPeriodForm((f) => ({ ...f, start_date: e.target.value }))}
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="period-end">End Date</Label>
 <Input
 id="period-end"
 type="date"
 value={periodForm.end_date}
 onChange={(e) => setPeriodForm((f) => ({ ...f, end_date: e.target.value }))}
 />
 </div>
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setPeriodDialogOpen(false)} disabled={periodSaving}>Cancel</Button>
 <Button onClick={savePeriod} disabled={periodSaving || !periodForm.start_date || !periodForm.end_date}>
 {periodSaving ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : null}
 Create
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
