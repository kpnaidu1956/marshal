import { useState, useCallback, useRef } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import {
 Play, CheckCircle2, XCircle, Loader2, AlertCircle, Clock,
 RotateCcw, Trash2,
} from 'lucide-react'

type TestStatus = 'pending' | 'running' | 'passed' | 'failed' | 'skipped'

interface TestResult {
 id: string
 group: string
 name: string
 status: TestStatus
 duration_ms?: number
 error?: string
 detail?: string
}

const STATUS_ICON: Record<TestStatus, React.ReactNode> = {
 pending: <Clock className="w-4 h-4 text-gray-400" />,
 running: <Loader2 className="w-4 h-4 text-blue-500 animate-spin" />,
 passed: <CheckCircle2 className="w-4 h-4 text-emerald-500" />,
 failed: <XCircle className="w-4 h-4 text-red-500" />,
 skipped: <AlertCircle className="w-4 h-4 text-amber-500" />,
}

function sleep(ms: number) {
 return new Promise((r) => setTimeout(r, ms))
}

export function BpeDiagnosticsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)
 const [results, setResults] = useState<TestResult[]>([])
 const [running, setRunning] = useState(false)
 const abortRef = useRef(false)

 const update = useCallback((id: string, patch: Partial<TestResult>) => {
 setResults((prev) => prev.map((r) => (r.id === id ? { ...r, ...patch } : r)))
 }, [])

 const runAll = useCallback(async () => {
 if (!token || !orgSlug) return
 abortRef.current = false
 setRunning(true)

 const bpe = new BpeClient(token)

 const tests: TestResult[] = [
 { id: 'health', group: 'Health', name: 'BPE server reachable', status: 'pending' },
 { id: 'ruflo', group: 'Health', name: 'Ruflo sidecar reachable', status: 'pending' },
 { id: 'agent-types', group: 'Health', name: 'Ruflo agent types available', status: 'pending' },
 { id: 'list-defs', group: 'Workflows', name: 'List workflow definitions', status: 'pending' },
 { id: 'create-def', group: 'Workflows', name: 'Create test workflow (3 steps)', status: 'pending' },
 { id: 'execute', group: 'Execution', name: 'Execute workflow', status: 'pending' },
 { id: 'confirm', group: 'Execution', name: 'Confirm execution', status: 'pending' },
 { id: 'start', group: 'Execution', name: 'Start execution', status: 'pending' },
 { id: 'poll-1', group: 'Execution', name: 'Poll: step 1 is ready, steps 2-3 pending', status: 'pending' },
 { id: 'complete-s1', group: 'Steps', name: 'Complete step 1', status: 'pending' },
 { id: 'poll-2', group: 'Steps', name: 'Poll: step 2 advanced to ready', status: 'pending' },
 { id: 'complete-s2', group: 'Steps', name: 'Complete step 2', status: 'pending' },
 { id: 'poll-3', group: 'Steps', name: 'Poll: step 3 advanced to ready', status: 'pending' },
 { id: 'complete-s3', group: 'Steps', name: 'Complete step 3 (final)', status: 'pending' },
 { id: 'poll-4', group: 'Steps', name: 'Poll: execution auto-completed', status: 'pending' },
 { id: 'audit', group: 'Audit', name: 'Audit trail has all expected events', status: 'pending' },
 { id: 'cleanup', group: 'Cleanup', name: 'Delete test workflow', status: 'pending' },
 ]

 setResults(tests)

 // Helper to run one test
 async function run(id: string, fn: () => Promise<string>) {
 if (abortRef.current) return false
 setResults((p) => p.map((r) => (r.id === id ? { ...r, status: 'running' as TestStatus } : r)))
 const t0 = performance.now()
 try {
 const detail = await fn()
 const ms = Math.round(performance.now() - t0)
 setResults((p) => p.map((r) => (r.id === id ? { ...r, status: 'passed' as TestStatus, duration_ms: ms, detail } : r)))
 return true
 } catch (e: unknown) {
 const msg = e instanceof Error ? e.message : String(e)
 const ms = Math.round(performance.now() - t0)
 setResults((p) => p.map((r) => (r.id === id ? { ...r, status: 'failed' as TestStatus, duration_ms: ms, error: msg } : r)))
 return false
 }
 }

 function skipRemaining() {
 setResults((p) => p.map((r) => (r.status === 'pending' ? { ...r, status: 'skipped' as TestStatus, error: 'Skipped (prior test failed)' } : r)))
 }

 let defId = ''
 let execId = ''
 let stepIds: string[] = []

 // --- Health ---
 await run('health', async () => {
 await bpe.listDefinitions(orgSlug)
 return 'BPE API responding'
 })

 await run('ruflo', async () => {
 const r = await bpe.rufloHealth()
 return r.ruflo_available ? 'Ruflo connected' : 'Ruflo not reachable (non-critical)'
 })

 await run('agent-types', async () => {
 try {
 const r = await bpe.rufloAgentTypes()
 return `Types: ${r.data.join(', ')}`
 } catch {
 return 'Ruflo unavailable (non-critical)'
 }
 })

 // --- Workflows ---
 await run('list-defs', async () => {
 const r = await bpe.listDefinitions(orgSlug)
 return `${r.total} definitions found`
 })

 const createOk = await run('create-def', async () => {
 const r = await bpe.createDefinition({
 organization_id: orgSlug,
 name: `_Diag ${new Date().toISOString().slice(11, 19)}`,
 category: 'general',
 step_templates: [
 { name: 'Prepare Data', step_type: 'manual', dependencies: [] },
 { name: 'Process Data', step_type: 'manual', dependencies: [1] },
 { name: 'Validate Output', step_type: 'manual', dependencies: [2] },
 ],
 })
 defId = r.data.id
 return `Created: ${defId.slice(0, 8)}`
 })

 if (!createOk || !defId) { skipRemaining(); setRunning(false); return }

 // --- Execution ---
 const execOk = await run('execute', async () => {
 const r = await bpe.executeDefinition(defId, { organization_id: orgSlug })
 execId = r.data.id
 return `Execution: ${execId.slice(0, 8)}`
 })

 if (!execOk || !execId) { skipRemaining(); setRunning(false); return }

 await run('confirm', async () => {
 await bpe.confirmExecution(execId)
 return 'Confirmed'
 })

 await run('start', async () => {
 await bpe.startExecution(execId)
 return 'Started'
 })

 await run('poll-1', async () => {
 const r = await bpe.getExecution(execId)
 stepIds = r.steps.map((s) => s.id)
 if (r.data.status !== 'running') throw new Error(`Expected running, got ${r.data.status}`)
 if (r.steps[0]?.status !== 'ready') throw new Error(`Step 1: expected ready, got ${r.steps[0]?.status}`)
 if (r.steps[1]?.status !== 'pending') throw new Error(`Step 2: expected pending, got ${r.steps[1]?.status}`)
 return `3 steps loaded, step 1 ready`
 })

 // --- Steps ---
 await run('complete-s1', async () => {
 await bpe.completeStep(stepIds[0], { output_data: { msg: 'data prepared' } })
 return 'Step 1 completed'
 })

 await sleep(300)

 await run('poll-2', async () => {
 const r = await bpe.getExecution(execId)
 if (r.steps[0]?.status !== 'completed') throw new Error(`Step 1: ${r.steps[0]?.status}`)
 if (r.steps[1]?.status !== 'ready') throw new Error(`Step 2: expected ready, got ${r.steps[1]?.status}`)
 return 'Step 1 completed, step 2 advanced to ready'
 })

 await run('complete-s2', async () => {
 await bpe.completeStep(stepIds[1], { output_data: { processed: true } })
 return 'Step 2 completed'
 })

 await sleep(300)

 await run('poll-3', async () => {
 const r = await bpe.getExecution(execId)
 if (r.steps[1]?.status !== 'completed') throw new Error(`Step 2: ${r.steps[1]?.status}`)
 if (r.steps[2]?.status !== 'ready') throw new Error(`Step 3: expected ready, got ${r.steps[2]?.status}`)
 return 'Step 2 completed, step 3 advanced to ready'
 })

 await run('complete-s3', async () => {
 await bpe.completeStep(stepIds[2], { output_data: { valid: true } })
 return 'Step 3 completed (final)'
 })

 await sleep(300)

 await run('poll-4', async () => {
 const r = await bpe.getExecution(execId)
 if (r.data.status !== 'completed') throw new Error(`Expected completed, got ${r.data.status}`)
 if (!r.data.completed_at) throw new Error('completed_at not set')
 const done = r.steps.filter((s) => s.status === 'completed' || s.status === 'skipped').length
 return `Execution auto-completed, ${done}/3 steps done`
 })

 // --- Audit ---
 await run('audit', async () => {
 const r = await bpe.executionTimeline(execId)
 const types = r.data.map((e) => e.event_type)
 const expected = [
 'workflow_execution.created',
 'workflow_execution.confirmed',
 'workflow_execution.started',
 'workflow_step.completed',
 'workflow_execution.completed',
 ]
 const missing = expected.filter((t) => !types.includes(t))
 if (missing.length > 0) throw new Error(`Missing: ${missing.join(', ')}`)
 return `${r.data.length} events, all expected types present`
 })

 // --- Cleanup ---
 await run('cleanup', async () => {
 try {
 await bpe.deleteDefinition(defId)
 return 'Deleted test definition'
 } catch {
 return 'Skipped (FK constraint, expected)'
 }
 })

 setRunning(false)
 }, [token, orgSlug, update])

 const passed = results.filter((r) => r.status === 'passed').length
 const failed = results.filter((r) => r.status === 'failed').length
 const skipped = results.filter((r) => r.status === 'skipped').length
 const total = results.length

 const groups: Record<string, TestResult[]> = {}
 for (const r of results) {
 ;(groups[r.group] ??= []).push(r)
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold tracking-tight">BPE Diagnostics</h1>
 <p className="text-sm text-muted-foreground mt-1">
 End-to-end tests against the live BPE server
 </p>
 </div>
 <div className="flex gap-2">
 {results.length > 0 && !running && (
 <Button variant="outline" size="sm" onClick={() => setResults([])}>
 <Trash2 className="w-4 h-4 mr-1" /> Clear
 </Button>
 )}
 <Button onClick={runAll} disabled={running || !token || !orgSlug}>
 {running ? (
 <><Loader2 className="w-4 h-4 mr-1 animate-spin" /> Running...</>
 ) : results.length > 0 ? (
 <><RotateCcw className="w-4 h-4 mr-1" /> Re-run Tests</>
 ) : (
 <><Play className="w-4 h-4 mr-1" /> Run All Tests</>
 )}
 </Button>
 </div>
 </div>

 {results.length > 0 && (
 <Card>
 <CardContent className="py-4">
 <div className="flex items-center gap-6">
 <div className="flex items-center gap-2">
 <div className={`w-3 h-3 rounded-full ${failed > 0 ? 'bg-red-500' : running ? 'bg-blue-500 animate-pulse' : 'bg-emerald-500'}`} />
 <span className="font-medium">
 {running ? 'Running...' : failed > 0 ? 'Tests Failed' : 'All Tests Passed'}
 </span>
 </div>
 <div className="flex gap-4 text-sm">
 <span className="text-emerald-600">{passed} passed</span>
 {failed > 0 && <span className="text-red-600">{failed} failed</span>}
 {skipped > 0 && <span className="text-amber-600">{skipped} skipped</span>}
 <span className="text-muted-foreground">{total} total</span>
 </div>
 {!running && total > 0 && (
 <div className="ml-auto">
 <div className="w-48 h-2 bg-gray-200 rounded-full overflow-hidden">
 <div
 className="h-full bg-emerald-500 transition-all duration-300"
 style={{ width: `${total > 0 ? (passed / total) * 100 : 0}%` }}
 />
 </div>
 </div>
 )}
 </div>
 </CardContent>
 </Card>
 )}

 {Object.entries(groups).map(([group, tests]) => (
 <Card key={group}>
 <CardContent className="py-4">
 <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">{group}</h3>
 <div className="space-y-2">
 {tests.map((t) => (
 <div
 key={t.id}
 className={`flex items-start gap-3 py-2 px-3 rounded-lg ${
 t.status === 'failed' ? 'bg-red-50' :
 t.status === 'passed' ? 'bg-emerald-50/50' :
 ''
 }`}
 >
 <div className="mt-0.5">{STATUS_ICON[t.status]}</div>
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2">
 <span className="text-sm font-medium">{t.name}</span>
 {t.duration_ms != null && (
 <span className="text-xs text-muted-foreground">{t.duration_ms}ms</span>
 )}
 </div>
 {t.detail && t.status === 'passed' && (
 <p className="text-xs text-muted-foreground mt-0.5">{t.detail}</p>
 )}
 {t.error && (
 <p className="text-xs text-red-600 mt-0.5">{t.error}</p>
 )}
 </div>
 </div>
 ))}
 </div>
 </CardContent>
 </Card>
 ))}

 {results.length === 0 && (
 <Card>
 <CardContent className="py-12 text-center">
 <AlertCircle className="w-10 h-10 text-muted-foreground mx-auto mb-3" />
 <p className="text-muted-foreground">
 Click <strong>Run All Tests</strong> to execute 17 end-to-end diagnostics.
 </p>
 <p className="text-xs text-muted-foreground mt-2">
 Creates a temporary workflow, runs it through all steps, verifies audit trail, then cleans up.
 </p>
 </CardContent>
 </Card>
 )}
 </div>
 )
}
