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
 DialogFooter,
 DialogDescription,
} from '@/components/ui/dialog'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { PostgRestClient } from '@/api/postgrest'
import { detectApiUrls } from '@/lib/config'
import { toast } from 'sonner'
import {
 Loader2,
 ShieldCheck,
 CheckCircle,
 XCircle,
 RefreshCw,
 Plus,
 Pencil,
 Trash2,
 Bot,
} from 'lucide-react'
import type { ApprovalRule, ApprovalRequest } from '@/models/bpe'

const STATUS_STYLES: Record<string, string> = {
 pending: 'bg-amber-100 text-amber-700',
 approved: 'bg-emerald-100 text-emerald-700',
 rejected: 'bg-red-100 text-red-700',
 cancelled: 'bg-gray-100 text-gray-700',
}

const EMPTY_RULE_FORM = {
 name: '',
 description: '',
 approval_type: 'single',
 approver_user_ids: '',
 required_approvals: 1,
 timeout_minutes: 0,
 allow_delegation: false,
}

type RuleFormState = typeof EMPTY_RULE_FORM

interface ResourceInfo {
 title: string
 description?: string | null
}

// ---------------------------------------------------------------------------
// Enrichment helpers — resolve UUIDs to names
// ---------------------------------------------------------------------------

/** Batch-fetch user display names for a set of UUIDs. */
async function fetchUserNames(
 userIds: string[],
 token: string | null,
): Promise<Map<string, string>> {
 const map = new Map<string, string>()
 if (userIds.length === 0) return map

 try {
 const { postgrestUrl, apiKey } = detectApiUrls()
 const client = new PostgRestClient(postgrestUrl, apiKey)
 const qs = `select=id,first_name,last_name,email&id=in.(${userIds.join(',')})`
 const rows = await client.get<{
 id: string
 first_name: string | null
 last_name: string | null
 email: string | null
 }>('users', qs, token)

 for (const u of rows) {
 const parts = [u.first_name, u.last_name].filter(Boolean)
 map.set(u.id, parts.length > 0 ? parts.join(' ') : u.email || 'Unknown')
 }
 } catch {
 // silently degrade — will fall back to "Marshal" or UUID
 }
 return map
}

/** Batch-fetch resource titles/descriptions for a set of resource IDs grouped by type. */
async function fetchResourceInfo(
 requests: ApprovalRequest[],
 token: string | null,
): Promise<Map<string, ResourceInfo>> {
 const map = new Map<string, ResourceInfo>()
 if (requests.length === 0) return map

 // Group resource IDs by type
 const byType = new Map<string, string[]>()
 for (const req of requests) {
 if (!byType.has(req.resource_type)) byType.set(req.resource_type, [])
 const ids = byType.get(req.resource_type)!
 if (!ids.includes(req.resource_id)) ids.push(req.resource_id)
 }

 const { postgrestUrl, apiKey } = detectApiUrls()
 const client = new PostgRestClient(postgrestUrl, apiKey)

 // Fetch from PostgREST tables for known types
 const tableMap: Record<string, string> = {
 task: 'tasks',
 goal: 'goals',
 }

 const fetches: Promise<void>[] = []

 for (const [type, ids] of byType.entries()) {
 const table = tableMap[type]
 if (table && ids.length > 0) {
 fetches.push(
 (async () => {
 try {
 const qs = `select=id,title,description&id=in.(${ids.join(',')})`
 const rows = await client.get<{
 id: string
 title: string
 description: string | null
 }>(table, qs, token)
 for (const row of rows) {
 map.set(row.id, { title: row.title, description: row.description })
 }
 } catch {
 // ignore — will show fallback
 }
 })(),
 )
 }
 }

 await Promise.all(fetches)
 return map
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function BpeApprovalsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [tab, setTab] = useState<'pending' | 'rules' | 'all'>('pending')
 const [rules, setRules] = useState<ApprovalRule[]>([])
 const [requests, setRequests] = useState<ApprovalRequest[]>([])
 const [pendingRequests, setPendingRequests] = useState<ApprovalRequest[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [deciding, setDeciding] = useState<string | null>(null)

 // Enrichment maps
 const [userNames, setUserNames] = useState<Map<string, string>>(new Map())
 const [resourceInfo, setResourceInfo] = useState<Map<string, ResourceInfo>>(new Map())

 // Create/Edit rule dialog
 const [ruleDialogOpen, setRuleDialogOpen] = useState(false)
 const [editingRuleId, setEditingRuleId] = useState<string | null>(null)
 const [ruleForm, setRuleForm] = useState<RuleFormState>({ ...EMPTY_RULE_FORM })
 const [ruleSaving, setRuleSaving] = useState(false)

 // Delete rule confirm
 const [deleteRuleId, setDeleteRuleId] = useState<string | null>(null)
 const [deleteRuleName, setDeleteRuleName] = useState('')

 // Decision dialog
 const [decisionTarget, setDecisionTarget] = useState<ApprovalRequest | null>(null)
 const [decisionComment, setDecisionComment] = useState('')

 /** Resolve requester name — user name or "Marshal" for automation. */
 const requesterName = useCallback(
 (userId: string): { name: string; isAutomation: boolean } => {
 const name = userNames.get(userId)
 if (name) return { name, isAutomation: false }
 return { name: 'Marshal', isAutomation: true }
 },
 [userNames],
 )

 /** Get enriched resource info for a request. */
 const getResourceDisplay = useCallback(
 (req: ApprovalRequest): ResourceInfo | null => {
 return resourceInfo.get(req.resource_id) || null
 },
 [resourceInfo],
 )

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const [r, req, pending] = await Promise.all([
 client.listRules(orgSlug),
 client.listRequests(orgSlug),
 client.pendingForMe(orgSlug),
 ])
 setRules(r.data)
 setRequests(req.data)
 setPendingRequests(pending.data)

 // Enrich: collect unique user IDs and fetch names + resource info
 const allReqs = [...req.data, ...pending.data]
 const uniqueUserIds = [...new Set(allReqs.map((r) => r.requested_by).filter(Boolean))]

 const [names, resources] = await Promise.all([
 fetchUserNames(uniqueUserIds, token),
 fetchResourceInfo(allReqs, token),
 ])
 setUserNames(names)
 setResourceInfo(resources)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load approvals')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 // --- Decision handling with comment dialog ---

 const openDecisionDialog = (req: ApprovalRequest) => {
 setDecisionTarget(req)
 setDecisionComment('')
 }

 const submitDecision = async (decision: 'approved' | 'rejected') => {
 if (!token || !decisionTarget) return
 setDeciding(decisionTarget.id)
 try {
 const client = new BpeClient(token)
 await client.decideRequest(decisionTarget.id, {
 decision,
 comment: decisionComment.trim() || null,
 })
 toast.success(decision === 'approved' ? 'Request approved' : 'Request rejected')
 setDecisionTarget(null)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Decision failed')
 } finally {
 setDeciding(null)
 }
 }

 // --- Create / Edit rule ---

 const openCreateRule = () => {
 setEditingRuleId(null)
 setRuleForm({ ...EMPTY_RULE_FORM })
 setRuleDialogOpen(true)
 }

 const openEditRule = (rule: ApprovalRule) => {
 setEditingRuleId(rule.id)
 setRuleForm({
 name: rule.name,
 description: rule.description || '',
 approval_type: rule.approval_type,
 approver_user_ids: rule.required_approvers.join(', '),
 required_approvals: rule.min_approvals,
 timeout_minutes: 0,
 allow_delegation: false,
 })
 setRuleDialogOpen(true)
 }

 const saveRule = async () => {
 if (!token || !orgSlug) return
 setRuleSaving(true)
 try {
 const client = new BpeClient(token)
 const approverIds = ruleForm.approver_user_ids
 .split(',')
 .map((s) => s.trim())
 .filter(Boolean)

 const body = {
 organization_id: orgSlug,
 name: ruleForm.name,
 description: ruleForm.description || null,
 approval_type: ruleForm.approval_type,
 required_approvers: approverIds,
 min_approvals: ruleForm.required_approvals,
 timeout_minutes: ruleForm.timeout_minutes,
 allow_delegation: ruleForm.allow_delegation,
 }

 if (editingRuleId) {
 await client.updateRule(editingRuleId, body)
 toast.success('Rule updated')
 } else {
 await client.createRule(body)
 toast.success('Rule created')
 }
 setRuleDialogOpen(false)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to save rule')
 } finally {
 setRuleSaving(false)
 }
 }

 // --- Delete rule ---

 const confirmDeleteRule = (rule: ApprovalRule) => {
 setDeleteRuleId(rule.id)
 setDeleteRuleName(rule.name)
 }

 const executeDeleteRule = async () => {
 if (!token || !deleteRuleId) return
 try {
 const client = new BpeClient(token)
 await client.deleteRule(deleteRuleId)
 toast.success('Rule deleted')
 setDeleteRuleId(null)
 await fetchData()
 } catch (err) {
 toast.error(err instanceof Error ? err.message : 'Failed to delete rule')
 }
 }

 // --- Render helpers ---

 /** Render the resource header (title + description) for an approval request. */
 function ResourceHeader({ req }: { req: ApprovalRequest }) {
 const info = getResourceDisplay(req)
 const typeLabel = req.resource_type.charAt(0).toUpperCase() + req.resource_type.slice(1).replace(/_/g, ' ')

 return (
 <div className="min-w-0">
 <div className="flex items-center gap-2 mb-0.5">
 <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_STYLES[req.status] || STATUS_STYLES.pending}`}>
 {req.status}
 </span>
 <Badge variant="outline" className="text-xs">{typeLabel}</Badge>
 </div>
 {info ? (
 <>
 <h3 className="font-semibold text-sm text-gray-900 truncate">
 {info.title}
 </h3>
 {info.description && (
 <p className="text-xs text-gray-500 line-clamp-2 mt-0.5">
 {info.description}
 </p>
 )}
 </>
 ) : (
 <p className="text-xs text-gray-500 font-mono">{req.resource_id.slice(0, 12)}...</p>
 )}
 </div>
 )
 }

 /** Render the requester line. */
 function RequesterLine({ req }: { req: ApprovalRequest }) {
 const { name, isAutomation } = requesterName(req.requested_by)
 return (
 <span className="text-xs text-gray-500">
 {isAutomation ? (
 <span className="inline-flex items-center gap-1">
 <Bot className="w-3 h-3" />
 <span className="font-medium text-indigo-500">{name}</span>
 </span>
 ) : (
 <span>{name}</span>
 )}
 {' '}&middot;{' '}
 {new Date(req.created_at).toLocaleString()}
 </span>
 )
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view approvals.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <h1 className="text-2xl font-bold text-gray-900">Approvals</h1>
 <Button variant="outline" size="sm" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Refresh</Button>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {/* Tabs */}
 <div className="flex gap-2 border-b border-gray-200 pb-1">
 {(['pending', 'all', 'rules'] as const).map((t) => (
 <button
 key={t}
 onClick={() => setTab(t)}
 className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors ${
 tab === t
 ? 'text-indigo-600 border-b-2 border-indigo-600'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 {t === 'pending' ? `Pending (${pendingRequests.length})` : t === 'all' ? `All Requests (${requests.length})` : `Rules (${rules.length})`}
 </button>
 ))}
 </div>

 {/* Pending Tab */}
 {tab === 'pending' && (
 <div className="space-y-3">
 {pendingRequests.length === 0 ? (
 <div className="text-center py-12">
 <ShieldCheck className="w-12 h-12 mx-auto text-emerald-400 mb-3" />
 <p className="text-gray-500">No pending approvals</p>
 </div>
 ) : pendingRequests.map((req) => (
 <Card key={req.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-start justify-between gap-4">
 <div className="flex-1 min-w-0">
 <ResourceHeader req={req} />
 <div className="mt-2">
 <RequesterLine req={req} />
 </div>
 </div>
 <div className="flex-shrink-0">
 <Button
 size="sm"
 onClick={() => openDecisionDialog(req)}
 disabled={deciding === req.id}
 >
 <CheckCircle className="w-4 h-4 mr-1" />Decide
 </Button>
 </div>
 </div>
 </CardContent>
 </Card>
 ))}
 </div>
 )}

 {/* All Requests Tab */}
 {tab === 'all' && (
 <div className="space-y-3">
 {requests.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-8">No approval requests</p>
 ) : requests.map((req) => {
 const info = getResourceDisplay(req)
 const { name: reqName, isAutomation } = requesterName(req.requested_by)
 const typeLabel = req.resource_type.charAt(0).toUpperCase() + req.resource_type.slice(1).replace(/_/g, ' ')

 return (
 <Card key={req.id}>
 <CardContent className="pt-3 pb-3">
 <div className="flex items-start justify-between gap-3">
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2 mb-0.5">
 <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_STYLES[req.status] || STATUS_STYLES.pending}`}>
 {req.status}
 </span>
 <Badge variant="outline" className="text-xs">{typeLabel}</Badge>
 </div>
 {info ? (
 <p className="text-sm font-medium text-gray-900 truncate">{info.title}</p>
 ) : (
 <p className="text-xs text-gray-500 font-mono">{req.resource_id.slice(0, 12)}...</p>
 )}
 {info?.description && (
 <p className="text-xs text-gray-500 line-clamp-1 mt-0.5">{info.description}</p>
 )}
 </div>
 <div className="text-right flex-shrink-0">
 <div className="text-xs text-gray-600">
 {isAutomation ? (
 <span className="inline-flex items-center gap-1">
 <Bot className="w-3 h-3" />
 <span className="text-indigo-500 font-medium">Marshal</span>
 </span>
 ) : (
 reqName
 )}
 </div>
 <div className="text-xs text-gray-500 mt-0.5">
 {new Date(req.created_at).toLocaleString()}
 </div>
 </div>
 </div>
 </CardContent>
 </Card>
 )
 })}
 </div>
 )}

 {/* Rules Tab */}
 {tab === 'rules' && (
 <div className="space-y-4">
 <div className="flex justify-end">
 <Button size="sm" onClick={openCreateRule}>
 <Plus className="w-4 h-4 mr-1" />Create Rule
 </Button>
 </div>
 <div className="grid gap-4">
 {rules.length === 0 ? (
 <p className="text-sm text-gray-500 text-center py-8">No approval rules configured</p>
 ) : rules.map((rule) => (
 <Card key={rule.id}>
 <CardContent className="pt-4 pb-4">
 <div className="flex items-start justify-between">
 <div>
 <h3 className="font-semibold text-gray-900">{rule.name}</h3>
 {rule.description && <p className="text-sm text-gray-500 mt-1">{rule.description}</p>}
 <div className="flex items-center gap-3 mt-2 text-xs text-gray-500">
 <Badge variant="secondary">{rule.resource_type}</Badge>
 <Badge variant="outline">{rule.approval_type}</Badge>
 <span>Min approvals: {rule.min_approvals}</span>
 <span>{rule.required_approvers.length} approvers</span>
 </div>
 </div>
 <div className="flex items-center gap-2">
 <Badge variant={rule.is_active ? 'default' : 'secondary'}>
 {rule.is_active ? 'Active' : 'Inactive'}
 </Badge>
 <Button variant="ghost" size="sm" onClick={() => openEditRule(rule)}>
 <Pencil className="w-4 h-4" />
 </Button>
 <Button variant="ghost" size="sm" onClick={() => confirmDeleteRule(rule)}>
 <Trash2 className="w-4 h-4 text-red-500" />
 </Button>
 </div>
 </div>
 </CardContent>
 </Card>
 ))}
 </div>
 </div>
 )}

 {/* Create / Edit Rule Dialog */}
 <Dialog open={ruleDialogOpen} onOpenChange={setRuleDialogOpen}>
 <DialogContent className="sm:max-w-lg">
 <DialogHeader>
 <DialogTitle>{editingRuleId ? 'Edit Rule' : 'Create Approval Rule'}</DialogTitle>
 <DialogDescription>
 {editingRuleId ? 'Update the approval rule configuration.' : 'Define a new approval rule for your organization.'}
 </DialogDescription>
 </DialogHeader>
 <div className="space-y-4 py-2">
 <div className="space-y-1.5">
 <Label htmlFor="rule-name">Name</Label>
 <Input
 id="rule-name"
 value={ruleForm.name}
 onChange={(e) => setRuleForm((f) => ({ ...f, name: e.target.value }))}
 placeholder="e.g. Budget Approval"
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="rule-desc">Description</Label>
 <Textarea
 id="rule-desc"
 value={ruleForm.description}
 onChange={(e) => setRuleForm((f) => ({ ...f, description: e.target.value }))}
 placeholder="Optional description"
 rows={2}
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="rule-type">Approval Type</Label>
 <select
 id="rule-type"
 value={ruleForm.approval_type}
 onChange={(e) => setRuleForm((f) => ({ ...f, approval_type: e.target.value }))}
 className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
 >
 <option value="single">Single</option>
 <option value="quorum">Quorum</option>
 <option value="unanimous">Unanimous</option>
 <option value="sequential">Sequential</option>
 </select>
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="rule-approvers">Approver User IDs (comma-separated)</Label>
 <Textarea
 id="rule-approvers"
 value={ruleForm.approver_user_ids}
 onChange={(e) => setRuleForm((f) => ({ ...f, approver_user_ids: e.target.value }))}
 placeholder="uuid-1, uuid-2, ..."
 rows={2}
 />
 </div>
 <div className="grid grid-cols-2 gap-4">
 <div className="space-y-1.5">
 <Label htmlFor="rule-min">Required Approvals</Label>
 <Input
 id="rule-min"
 type="number"
 min={1}
 value={ruleForm.required_approvals}
 onChange={(e) => setRuleForm((f) => ({ ...f, required_approvals: parseInt(e.target.value, 10) || 1 }))}
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="rule-timeout">Timeout (minutes, 0 = none)</Label>
 <Input
 id="rule-timeout"
 type="number"
 min={0}
 value={ruleForm.timeout_minutes}
 onChange={(e) => setRuleForm((f) => ({ ...f, timeout_minutes: parseInt(e.target.value, 10) || 0 }))}
 />
 </div>
 </div>
 <div className="flex items-center gap-2">
 <input
 id="rule-delegation"
 type="checkbox"
 checked={ruleForm.allow_delegation}
 onChange={(e) => setRuleForm((f) => ({ ...f, allow_delegation: e.target.checked }))}
 className="rounded border-gray-300"
 />
 <Label htmlFor="rule-delegation" className="text-sm font-normal">Allow delegation</Label>
 </div>
 </div>
 <DialogFooter>
 <Button variant="outline" onClick={() => setRuleDialogOpen(false)} disabled={ruleSaving}>Cancel</Button>
 <Button onClick={saveRule} disabled={ruleSaving || !ruleForm.name.trim()}>
 {ruleSaving ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : null}
 {editingRuleId ? 'Save Changes' : 'Create Rule'}
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>

 {/* Delete Rule Confirm */}
 <ConfirmDialog
 open={!!deleteRuleId}
 onOpenChange={(open) => { if (!open) setDeleteRuleId(null) }}
 title="Delete Rule"
 description={`Are you sure you want to delete "${deleteRuleName}"? This action cannot be undone.`}
 confirmLabel="Delete"
 variant="danger"
 onConfirm={executeDeleteRule}
 />

 {/* Decision Dialog (Approve / Reject with notes) */}
 <Dialog open={!!decisionTarget} onOpenChange={(open) => { if (!open) setDecisionTarget(null) }}>
 <DialogContent className="sm:max-w-md">
 <DialogHeader>
 <DialogTitle>Approval Decision</DialogTitle>
 <DialogDescription>
 {decisionTarget && (() => {
 const info = getResourceDisplay(decisionTarget)
 const typeLabel = decisionTarget.resource_type.charAt(0).toUpperCase() + decisionTarget.resource_type.slice(1).replace(/_/g, ' ')
 const { name: reqName, isAutomation } = requesterName(decisionTarget.requested_by)
 return (
 <>
 <span className="font-medium">{typeLabel}</span>
 {info ? (
 <>: <span className="font-semibold text-gray-900">{info.title}</span></>
 ) : (
 <>{' '}&middot; <span className="font-mono text-xs">{decisionTarget.resource_id.slice(0, 12)}...</span></>
 )}
 <br />
 <span className="text-xs">
 Requested by{' '}
 {isAutomation ? (
 <span className="text-indigo-500 font-medium">Marshal</span>
 ) : (
 <span className="font-medium">{reqName}</span>
 )}
 </span>
 </>
 )
 })()}
 </DialogDescription>
 </DialogHeader>
 {/* Show description in decision dialog */}
 {decisionTarget && (() => {
 const info = getResourceDisplay(decisionTarget)
 if (!info?.description) return null
 return (
 <div className="bg-gray-50 rounded-md p-3 text-sm text-gray-600">
 {info.description}
 </div>
 )
 })()}
 <div className="space-y-3 py-2">
 <div className="space-y-1.5">
 <Label htmlFor="decision-comment">Notes (optional)</Label>
 <Textarea
 id="decision-comment"
 value={decisionComment}
 onChange={(e) => setDecisionComment(e.target.value)}
 placeholder="Add a comment for this decision..."
 rows={3}
 />
 </div>
 </div>
 <DialogFooter className="gap-2 sm:gap-0">
 <Button
 variant="destructive"
 onClick={() => submitDecision('rejected')}
 disabled={deciding === decisionTarget?.id}
 >
 {deciding === decisionTarget?.id ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <XCircle className="w-4 h-4 mr-1" />}
 Reject
 </Button>
 <Button
 onClick={() => submitDecision('approved')}
 disabled={deciding === decisionTarget?.id}
 >
 {deciding === decisionTarget?.id ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <CheckCircle className="w-4 h-4 mr-1" />}
 Approve
 </Button>
 </DialogFooter>
 </DialogContent>
 </Dialog>
 </div>
 )
}
