import { useState, useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { detectApiUrls } from '@/lib/config'
import { Loader2, Activity, GitBranch, ShieldCheck, Bell, BarChart3, Zap, RefreshCw } from 'lucide-react'
import type { BpeDashboard, WorkflowPerformanceItem } from '@/models/bpe'

export function BpeDashboardPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)
 const availableOrgs = useOrgStore((s) => s.availableOrgs)
 const setAvailableOrgs = useOrgStore((s) => s.setAvailableOrgs)
 const setCurrentOrg = useOrgStore((s) => s.setCurrentOrg)
 const navigate = useNavigate()

 // Auto-fetch orgs if missing (e.g. user logged in before org code was deployed)
 useEffect(() => {
 if (!token || availableOrgs.length > 0) return
 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = { Authorization: `Bearer ${token}` }
 if (apiKey) headers['apikey'] = apiKey
 fetch(`${ragUrl}/api/auth/organizations`, { headers })
 .then((r) => r.ok ? r.json() : [])
 .then((orgs) => {
 if (orgs.length) {
 setAvailableOrgs(orgs)
 setCurrentOrg(orgs[0])
 }
 })
 .catch(() => {})
 }, [token, availableOrgs.length, setAvailableOrgs, setCurrentOrg])

 const [dashboard, setDashboard] = useState<BpeDashboard | null>(null)
 const [performance, setPerformance] = useState<WorkflowPerformanceItem[]>([])
 const [unread, setUnread] = useState(0)
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const [dash, perf, notif] = await Promise.all([
 client.dashboard(orgSlug),
 client.workflowPerformance(orgSlug),
 client.unreadCount(orgSlug),
 ])
 setDashboard(dash.data)
 setPerformance(perf.data)
 setUnread(notif.unread_count)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load BPE dashboard')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view the BPE dashboard.</p>
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

 if (error) {
 return (
 <div className="text-center py-12">
 <p className="text-red-600 mb-4">{error}</p>
 <Button variant="outline" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Retry</Button>
 </div>
 )
 }

 const d = dashboard!

 const cards = [
 { label: 'Entities', value: d.entities, icon: Activity, color: 'text-blue-600', bg: 'bg-blue-50', to: '/bpe/entities' },
 { label: 'Workflow Definitions', value: d.workflow_definitions, icon: GitBranch, color: 'text-emerald-600', bg: 'bg-emerald-50', to: '/bpe/workflows' },
 { label: 'Active Workflows', value: d.workflow_executions.active, icon: Zap, color: 'text-amber-600', bg: 'bg-amber-50', to: '/bpe/workflows' },
 { label: 'Completed Workflows', value: d.workflow_executions.completed, icon: BarChart3, color: 'text-indigo-600', bg: 'bg-indigo-50', to: '/bpe/workflows' },
 { label: 'Pending Approvals', value: d.pending_approvals, icon: ShieldCheck, color: 'text-orange-600', bg: 'bg-orange-50', to: '/bpe/approvals' },
 { label: 'Unread Notifications', value: unread, icon: Bell, color: 'text-rose-600', bg: 'bg-rose-50', to: '/bpe/notifications' },
 ]

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold text-gray-900">Business Process Engine</h1>
 <p className="text-sm text-gray-500 mt-1">
 {d.audit_events_24h} audit events in the last 24 hours &middot; {d.learned_sequences} learned sequences
 </p>
 </div>
 <Button variant="outline" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4 mr-2" />Refresh
 </Button>
 </div>

 {/* Summary Cards */}
 <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
 {cards.map((c) => (
 <Card
 key={c.label}
 className="cursor-pointer hover:shadow-md transition-shadow"
 onClick={() => navigate(c.to)}
 >
 <CardContent className="pt-4 pb-4">
 <div className={`w-10 h-10 rounded-lg ${c.bg} flex items-center justify-center mb-3`}>
 <c.icon className={`w-5 h-5 ${c.color}`} />
 </div>
 <p className="text-2xl font-bold text-gray-900">{c.value}</p>
 <p className="text-xs text-gray-500 mt-1">{c.label}</p>
 </CardContent>
 </Card>
 ))}
 </div>

 {/* Workflow Performance Table */}
 <Card>
 <CardHeader>
 <CardTitle className="text-lg">Workflow Performance</CardTitle>
 </CardHeader>
 <CardContent>
 {performance.length === 0 ? (
 <p className="text-sm text-gray-500 py-4 text-center">No workflow definitions yet</p>
 ) : (
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b border-gray-200">
 <th className="text-left py-2 px-3 font-medium text-gray-500">Name</th>
 <th className="text-left py-2 px-3 font-medium text-gray-500">Category</th>
 <th className="text-right py-2 px-3 font-medium text-gray-500">Executions</th>
 <th className="text-right py-2 px-3 font-medium text-gray-500">Completed</th>
 <th className="text-right py-2 px-3 font-medium text-gray-500">Failed</th>
 <th className="text-right py-2 px-3 font-medium text-gray-500">Success Rate</th>
 <th className="text-right py-2 px-3 font-medium text-gray-500">Avg Duration</th>
 </tr>
 </thead>
 <tbody>
 {performance.map((p) => (
 <tr key={p.name} className="border-b border-gray-100 hover:bg-gray-50">
 <td className="py-2 px-3 font-medium text-gray-900">{p.name}</td>
 <td className="py-2 px-3">
 <Badge variant="secondary">{p.category}</Badge>
 </td>
 <td className="py-2 px-3 text-right">{p.execution_count}</td>
 <td className="py-2 px-3 text-right text-emerald-600">{p.completed}</td>
 <td className="py-2 px-3 text-right text-red-600">{p.failed}</td>
 <td className="py-2 px-3 text-right">
 <Badge variant={p.success_rate >= 0.8 ? 'default' : p.success_rate >= 0.5 ? 'secondary' : 'destructive'}>
 {(p.success_rate * 100).toFixed(0)}%
 </Badge>
 </td>
 <td className="py-2 px-3 text-right text-gray-500">
 {p.avg_duration_minutes != null ? `${p.avg_duration_minutes.toFixed(1)}m` : '—'}
 </td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 )}
 </CardContent>
 </Card>
 </div>
 )
}
