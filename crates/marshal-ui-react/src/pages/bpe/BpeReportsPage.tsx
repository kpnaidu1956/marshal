import { useState, useEffect, useCallback } from 'react'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'
import { Loader2, FileText, Play, Trash2, RefreshCw, Plus, Download } from 'lucide-react'
import type { ReportTemplate, ReportResult } from '@/models/bpe'

export function BpeReportsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [templates, setTemplates] = useState<ReportTemplate[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [runningId, setRunningId] = useState<string | null>(null)
 const [result, setResult] = useState<ReportResult | null>(null)

 // Create form
 const [showCreate, setShowCreate] = useState(false)
 const [createLoading, setCreateLoading] = useState(false)
 const [formName, setFormName] = useState('')
 const [formDesc, setFormDesc] = useState('')
 const [formCategory, setFormCategory] = useState('general')
 const [formSql, setFormSql] = useState('')

 // Delete confirm
 const [deleteId, setDeleteId] = useState<string | null>(null)

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.listTemplates(orgSlug)
 setTemplates(res.data)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load templates')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 const runReport = async (id: string) => {
 if (!token || !orgSlug) return
 setRunningId(id)
 setResult(null)
 setError(null)
 try {
 const client = new BpeClient(token)
 const res = await client.runReport(id, { organization_id: orgSlug })
 setResult(res.data)
 toast.success(`Report generated — ${res.data.row_count} rows`)
 } catch (err) {
 const msg = err instanceof Error ? err.message : 'Report failed'
 setError(msg)
 toast.error(msg)
 } finally {
 setRunningId(null)
 }
 }

 const handleDelete = async () => {
 if (!token || !deleteId) return
 try {
 const client = new BpeClient(token)
 await client.deleteTemplate(deleteId)
 toast.success('Report template deleted')
 setResult(null)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Delete failed')
 }
 setDeleteId(null)
 }

 const handleCreate = async () => {
 if (!token || !orgSlug || !formName.trim() || !formSql.trim()) return
 setCreateLoading(true)
 try {
 const client = new BpeClient(token)
 await client.createTemplate({
 organization_id: orgSlug,
 name: formName.trim(),
 description: formDesc.trim() || undefined,
 category: formCategory,
 sql_template: formSql.trim(),
 })
 toast.success('Report template created')
 setShowCreate(false)
 setFormName('')
 setFormDesc('')
 setFormCategory('general')
 setFormSql('')
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Create failed')
 } finally {
 setCreateLoading(false)
 }
 }

 const exportCsv = () => {
 if (!result || result.rows.length === 0) return
 const headers = Object.keys(result.rows[0])
 const csvRows = [
 headers.join(','),
 ...result.rows.map((row) =>
 headers.map((h) => {
 const v = (row as Record<string, unknown>)[h]
 const s = v == null ? '' : String(v)
 return s.includes(',') || s.includes('"') ? `"${s.replace(/"/g, '""')}"` : s
 }).join(',')
 ),
 ]
 const blob = new Blob([csvRows.join('\n')], { type: 'text/csv' })
 const url = URL.createObjectURL(blob)
 const a = document.createElement('a')
 a.href = url
 a.download = `${result.template_name.replace(/\s+/g, '_')}.csv`
 a.click()
 URL.revokeObjectURL(url)
 toast.success('CSV downloaded')
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view reports.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Reports</h1>
 <div className="flex gap-2">
 <Button variant="outline" size="sm" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Refresh</Button>
 <Button size="sm" onClick={() => setShowCreate(true)}><Plus className="w-4 h-4 mr-2" />New Template</Button>
 </div>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {templates.length === 0 ? (
 <div className="text-center py-12">
 <FileText className="w-12 h-12 mx-auto text-gray-400 mb-3" />
 <p className="text-gray-500">No report templates</p>
 <p className="text-xs text-gray-500 mt-1">Create a template to run reports on your BPE data</p>
 </div>
 ) : (
 <div className="grid gap-4 md:grid-cols-2">
 {templates.map((t) => (
 <Card key={t.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-start justify-between">
 <div className="flex-1">
 <div className="flex items-center gap-2 mb-1">
 <h3 className="font-semibold text-gray-900">{t.name}</h3>
 <Badge variant="secondary">{t.category}</Badge>
 </div>
 {t.description && <p className="text-sm text-gray-500 mb-2">{t.description}</p>}
 </div>
 <div className="flex gap-2 ml-4">
 <Button
 size="sm"
 variant="outline"
 onClick={() => runReport(t.id)}
 disabled={runningId === t.id}
 >
 {runningId === t.id ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5 mr-1" />}
 Run
 </Button>
 <Button size="sm" variant="ghost" onClick={() => setDeleteId(t.id)}>
 <Trash2 className="w-3.5 h-3.5 text-red-500" />
 </Button>
 </div>
 </div>
 </CardContent>
 </Card>
 ))}
 </div>
 )}

 {/* Report Results */}
 {result && (
 <Card>
 <CardHeader>
 <div className="flex items-center justify-between">
 <CardTitle className="text-lg flex items-center gap-2">
 <FileText className="w-5 h-5" />
 {result.template_name} — {result.row_count} rows
 </CardTitle>
 {result.rows.length > 0 && (
 <Button size="sm" variant="outline" onClick={exportCsv}>
 <Download className="w-3.5 h-3.5 mr-1" />CSV
 </Button>
 )}
 </div>
 </CardHeader>
 <CardContent>
 {result.rows.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-4">No data returned</p>
 ) : (
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 {Object.keys(result.rows[0]).map((col) => (
 <th key={col} className="text-left py-2 px-3 font-medium text-gray-700">{col}</th>
 ))}
 </tr>
 </thead>
 <tbody>
 {result.rows.map((row, i) => (
 <tr key={i} className="border-b border-gray-100">
 {Object.values(row).map((val, j) => (
 <td key={j} className="py-2 px-3 text-gray-700">
 {val == null ? <span className="text-gray-500 italic">null</span> : String(val)}
 </td>
 ))}
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 )}
 <p className="text-xs text-gray-500 mt-3 text-right">
 Generated at {new Date(result.generated_at).toLocaleString()}
 </p>
 </CardContent>
 </Card>
 )}

 {/* Create Template Dialog */}
 <Dialog open={showCreate} onOpenChange={setShowCreate}>
 <DialogContent className="sm:max-w-lg">
 <DialogHeader>
 <DialogTitle>Create Report Template</DialogTitle>
 </DialogHeader>
 <div className="space-y-4">
 <div>
 <Label htmlFor="name">Name</Label>
 <Input id="name" value={formName} onChange={(e) => setFormName(e.target.value)} placeholder="Monthly Summary" />
 </div>
 <div>
 <Label htmlFor="desc">Description</Label>
 <Input id="desc" value={formDesc} onChange={(e) => setFormDesc(e.target.value)} placeholder="Optional description" />
 </div>
 <div>
 <Label htmlFor="category">Category</Label>
 <select
 id="category"
 value={formCategory}
 onChange={(e) => setFormCategory(e.target.value)}
 className="w-full border rounded-md px-3 py-2 text-sm bg-background"
 >
 {['general', 'workflow', 'entity', 'approval', 'integration', 'audit'].map((c) => (
 <option key={c} value={c}>{c}</option>
 ))}
 </select>
 </div>
 <div>
 <Label htmlFor="sql">SQL Template</Label>
 <Textarea
 id="sql"
 value={formSql}
 onChange={(e) => setFormSql(e.target.value)}
 placeholder="SELECT * FROM bpe.entities WHERE organization_id = $org_id LIMIT 100"
 rows={5}
 className="font-mono text-xs"
 />
 <p className="text-xs text-gray-500 mt-1">Use $org_id for organization filter. Must be a SELECT query.</p>
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setShowCreate(false)}>Cancel</Button>
 <Button onClick={handleCreate} disabled={createLoading || !formName.trim() || !formSql.trim()}>
 {createLoading ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
 Create
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete Confirm */}
 <ConfirmDialog
 open={!!deleteId}
 onOpenChange={(open) => !open && setDeleteId(null)}
 title="Delete Report Template"
 description="This will permanently delete this report template. This action cannot be undone."
 confirmLabel="Delete"
 variant="danger"
 onConfirm={handleDelete}
 />
 </div>
 )
}
