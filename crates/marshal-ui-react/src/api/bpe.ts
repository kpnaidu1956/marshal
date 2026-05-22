// BPE (Business Process Engine) API Client
// Talks to the BPE server at /bpe/api/* endpoints

import { detectApiUrls } from '@/lib/config'

function getBpeBaseUrl(): string {
 const hostname = window.location.hostname
 if (hostname === 'localhost' || hostname === '127.0.0.1') {
 return 'http://localhost:8090/bpe/api'
 }
 return `${window.location.origin}/bpe/api`
}

export class BpeClient {
 private baseUrl: string
 private token: string
 private apiKey: string | null

 constructor(token: string) {
 this.baseUrl = getBpeBaseUrl()
 this.token = token
 const { apiKey } = detectApiUrls()
 this.apiKey = apiKey ?? null
 }

 private async request<T>(
 method: string,
 path: string,
 body?: unknown,
 ): Promise<T> {
 const url = `${this.baseUrl}${path}`
 const headers: Record<string, string> = {
 Authorization: `Bearer ${this.token}`,
 'Content-Type': 'application/json',
 }
 if (this.apiKey) headers['apikey'] = this.apiKey

 const res = await fetch(url, {
 method,
 headers,
 body: body ? JSON.stringify(body) : undefined,
 })

 if (!res.ok) {
 const text = await res.text()
 let msg = `BPE API error ${res.status}`
 try {
 const j = JSON.parse(text)
 msg = j.error || j.message || msg
 } catch {
 if (text) msg = text
 }
 // Auto-logout on 401 (token expired or invalid)
 if (res.status === 401) {
 const { useAuthStore } = await import('@/stores/auth')
 useAuthStore.getState().logout()
 window.location.href = '/login'
 throw new Error('Session expired. Please log in again.')
 }
 throw new Error(msg)
 }

 return res.json()
 }

 // --- Dashboard & Reports ---

 async dashboard(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').BpeDashboard }>(
 'GET', `/reports/dashboard?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async workflowPerformance(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowPerformanceItem[], generated_at: string }>(
 'GET', `/reports/workflow-performance?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 // --- Workflow Definitions ---

 async listDefinitions(orgSlug: string, page = 1, perPage = 50) {
 return this.request<{ data: import('@/models/bpe').WorkflowDefinition[], page: number, per_page: number, total: number }>(
 'GET', `/workflows/definitions?organization_id=${encodeURIComponent(orgSlug)}&page=${page}&per_page=${perPage}`,
 )
 }

 async createDefinition(body: unknown) {
 return this.request<{ data: import('@/models/bpe').WorkflowDefinition }>(
 'POST', '/workflows/definitions', body,
 )
 }

 async updateDefinition(id: string, body: unknown) {
 return this.request<{ data: import('@/models/bpe').WorkflowDefinition }>(
 'PUT', `/workflows/definitions/${id}`, body,
 )
 }

 async deleteDefinition(id: string) {
 return this.request<{ status: string }>('DELETE', `/workflows/definitions/${id}`)
 }

 async executeDefinition(id: string, body: unknown) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution }>(
 'POST', `/workflows/definitions/${id}/execute`, body,
 )
 }

 // --- Workflow Executions ---

 async listExecutions(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution[] }>(
 'GET', `/workflows/executions?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async getExecution(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution; steps: import('@/models/bpe').WorkflowStep[] }>(
 'GET', `/workflows/executions/${id}`,
 )
 }

 async executionTimeline(id: string) {
 return this.request<{ data: import('@/models/bpe').TimelineEvent[] }>(
 'GET', `/workflows/executions/${id}/timeline`,
 )
 }

 async confirmExecution(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution }>(
 'POST', `/workflows/executions/${id}/confirm`, {},
 )
 }

 async startExecution(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution }>(
 'POST', `/workflows/executions/${id}/start`, {},
 )
 }

 async pauseExecution(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution }>(
 'POST', `/workflows/executions/${id}/pause`, {},
 )
 }

 async resumeExecution(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution }>(
 'POST', `/workflows/executions/${id}/resume`, {},
 )
 }

 async cancelExecution(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowExecution }>(
 'POST', `/workflows/executions/${id}/cancel`, {},
 )
 }

 // --- Workflow Steps ---

 async completeStep(id: string, body?: unknown) {
 return this.request<{ data: import('@/models/bpe').WorkflowStep }>(
 'POST', `/workflows/steps/${id}/complete`, body ?? {},
 )
 }

 async skipStep(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowStep }>(
 'POST', `/workflows/steps/${id}/skip`, {},
 )
 }

 async retryStep(id: string) {
 return this.request<{ data: import('@/models/bpe').WorkflowStep }>(
 'POST', `/workflows/steps/${id}/retry`, {},
 )
 }

 // --- Approval Rules ---

 async listRules(orgSlug: string, page = 1, perPage = 50) {
 return this.request<{ data: import('@/models/bpe').ApprovalRule[], page: number, per_page: number, total: number }>(
 'GET', `/approvals/rules?organization_id=${encodeURIComponent(orgSlug)}&page=${page}&per_page=${perPage}`,
 )
 }

 async createRule(body: unknown) {
 return this.request<{ data: import('@/models/bpe').ApprovalRule }>(
 'POST', '/approvals/rules', body,
 )
 }

 async updateRule(id: string, body: unknown) {
 return this.request<{ data: import('@/models/bpe').ApprovalRule }>(
 'PUT', `/approvals/rules/${id}`, body,
 )
 }

 async deleteRule(id: string) {
 return this.request<{ status: string }>('DELETE', `/approvals/rules/${id}`)
 }

 // --- Approval Requests ---

 async listRequests(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').ApprovalRequest[] }>(
 'GET', `/approvals/requests?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async pendingForMe(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').ApprovalRequest[] }>(
 'GET', `/approvals/pending?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async createRequest(body: unknown) {
 return this.request<{ data: import('@/models/bpe').ApprovalRequest }>(
 'POST', '/approvals/requests', body,
 )
 }

 async decideRequest(id: string, body: unknown) {
 return this.request<{ data: import('@/models/bpe').ApprovalDecision }>(
 'POST', `/approvals/requests/${id}/decide`, body,
 )
 }

 // --- Integration Credentials ---

 async listIntegrationTypes() {
 return this.request<{ data: import('@/models/bpe').IntegrationType[] }>(
 'GET', '/integrations/types',
 )
 }

 async listCredentials(orgSlug: string, page = 1, perPage = 50) {
 return this.request<{ data: import('@/models/bpe').IntegrationCredential[], page: number, per_page: number, total: number }>(
 'GET', `/integrations/credentials?organization_id=${encodeURIComponent(orgSlug)}&page=${page}&per_page=${perPage}`,
 )
 }

 async createCredential(body: unknown) {
 return this.request<{ data: import('@/models/bpe').IntegrationCredential }>(
 'POST', '/integrations/credentials', body,
 )
 }

 async deleteCredential(id: string) {
 return this.request<{ status: string }>('DELETE', `/integrations/credentials/${id}`)
 }

 // --- Ruflo AI Agent Integration ---

 async rufloHealth() {
 return this.request<{ ruflo_available: boolean }>(
 'GET', '/ruflo/health',
 )
 }

 async rufloAgentTypes() {
 return this.request<{ data: string[] }>(
 'GET', '/ruflo/agent-types',
 )
 }

 async rufloSpawnAgent(body: { agent_type: string; prompt: string; tools?: string[]; context?: unknown }) {
 return this.request<{ data: { agent_id: string; status: string; output: unknown; error: string | null } }>(
 'POST', '/ruflo/agent/spawn', body,
 )
 }

 async testCredential(id: string) {
 return this.request<{ success: boolean; message: string }>(
 'POST', `/integrations/credentials/${id}/test`, {},
 )
 }

 // --- Knowledge / Learned Sequences ---

 async listSequences(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').LearnedSequence[] }>(
 'GET', `/knowledge/sequences?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async suggestSequence(body: unknown) {
 return this.request<{ data: unknown[] }>(
 'POST', '/knowledge/suggest', body,
 )
 }

 async promoteSequence(id: string, body: unknown) {
 return this.request<{ data: import('@/models/bpe').WorkflowDefinition }>(
 'POST', `/knowledge/sequences/${id}/promote`, body,
 )
 }

 async deactivateSequence(id: string) {
 return this.request<{ status: string }>('DELETE', `/knowledge/sequences/${id}`)
 }

 // --- Report Templates ---

 async listTemplates(orgSlug: string, page = 1, perPage = 50) {
 return this.request<{ data: import('@/models/bpe').ReportTemplate[], page: number, per_page: number, total: number }>(
 'GET', `/reports/templates?organization_id=${encodeURIComponent(orgSlug)}&page=${page}&per_page=${perPage}`,
 )
 }

 async createTemplate(body: unknown) {
 return this.request<{ data: import('@/models/bpe').ReportTemplate }>(
 'POST', '/reports/templates', body,
 )
 }

 async runReport(id: string, body: unknown) {
 return this.request<{ data: import('@/models/bpe').ReportResult }>(
 'POST', `/reports/templates/${id}/run`, body,
 )
 }

 async deleteTemplate(id: string) {
 return this.request<{ status: string }>('DELETE', `/reports/templates/${id}`)
 }

 // --- Notifications ---

 async listNotifications(orgSlug: string, page = 1, perPage = 50) {
 return this.request<{ data: import('@/models/bpe').BpeNotification[], total: number, page: number, per_page: number }>(
 'GET', `/notifications?organization_id=${encodeURIComponent(orgSlug)}&page=${page}&per_page=${perPage}`,
 )
 }

 async unreadCount(orgSlug: string) {
 return this.request<{ unread_count: number }>(
 'GET', `/notifications/unread-count?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async markRead(ids: string[]) {
 return this.request<{ marked_read: number }>(
 'POST', '/notifications/mark-read', { notification_ids: ids },
 )
 }

 async markAllRead(orgSlug: string) {
 return this.request<{ marked_read: number }>(
 'POST', `/notifications/mark-all-read?organization_id=${encodeURIComponent(orgSlug)}`, {},
 )
 }

 // --- Audit ---

 async listAuditEvents(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').AuditEvent[] }>(
 'GET', `/audit/events?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 // --- Entities ---

 async listEntityTypes(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').BpeEntityType[] }>(
 'GET', `/entity-types?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async listEntities(orgSlug: string) {
 return this.request<{ data: import('@/models/bpe').BpeEntity[] }>(
 'GET', `/entities?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async createEntityType(body: unknown) {
 return this.request<{ data: import('@/models/bpe').BpeEntityType }>(
 'POST', '/entity-types', body,
 )
 }

 async deleteEntityType(id: string) {
 return this.request<{ status: string }>('DELETE', `/entity-types/${id}`)
 }

 async createEntity(body: unknown) {
 return this.request<{ data: import('@/models/bpe').BpeEntity }>(
 'POST', '/entities', body,
 )
 }

 async deleteEntity(id: string) {
 return this.request<{ status: string }>('DELETE', `/entities/${id}`)
 }

 // ── Timekeeping: Employees ──

 async tkListEmployees(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[], total: number, page: number, per_page: number }>(
 'GET', `/timekeeping/employees?${qs}`,
 )
 }

 async tkCreateEmployee(body: unknown) {
 return this.request<{ data: unknown }>('POST', '/timekeeping/employees', body)
 }

 async tkUpdateEmployee(id: string, body: unknown) {
 return this.request<{ data: unknown }>('PUT', `/timekeeping/employees/${id}`, body)
 }

 async tkDeleteEmployee(id: string) {
 return this.request<{ status: string }>('DELETE', `/timekeeping/employees/${id}`)
 }

 async tkImportEmployees(body: unknown) {
 return this.request<{ created: number, skipped: number, errors: string[] }>(
 'POST', '/timekeeping/employees/import', body,
 )
 }

 // ── Timekeeping: Stations ──

 async tkListStations(orgSlug: string) {
 return this.request<{ data: unknown[] }>(
 'GET', `/timekeeping/stations?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async tkCreateStation(body: unknown) {
 return this.request<{ data: unknown }>('POST', '/timekeeping/stations', body)
 }

 async tkUpdateStation(id: string, body: unknown) {
 return this.request<{ data: unknown }>('PUT', `/timekeeping/stations/${id}`, body)
 }

 // ── Timekeeping: Kelly Schedule ──

 async tkGetKellyConfig(orgSlug: string) {
 return this.request<{ data: unknown }>(
 'GET', `/timekeeping/kelly-config?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async tkUpsertKellyConfig(body: unknown) {
 return this.request<{ data: unknown }>('PUT', '/timekeeping/kelly-config', body)
 }

 async tkComputeSchedule(orgSlug: string, start: string, end: string) {
 return this.request<{ data: unknown[], config: unknown }>(
 'GET', `/timekeeping/kelly-schedule?organization_id=${encodeURIComponent(orgSlug)}&start=${start}&end=${end}`,
 )
 }

 // ── Timekeeping: Roster ──

 async tkGetRoster(orgSlug: string, date: string) {
 return this.request<{ data: unknown }>(
 'GET', `/timekeeping/roster?organization_id=${encodeURIComponent(orgSlug)}&date=${date}`,
 )
 }

 async tkGetRosterRange(orgSlug: string, start: string, end: string) {
 return this.request<{ data: unknown[] }>(
 'GET', `/timekeeping/roster/range?organization_id=${encodeURIComponent(orgSlug)}&start=${start}&end=${end}`,
 )
 }

 async tkGenerateRoster(body: unknown) {
 return this.request<{ created: number, skipped: number, alerts: string[] }>(
 'POST', '/timekeeping/roster/generate', body,
 )
 }

 async tkUpdateRoster(id: string, body: unknown) {
 return this.request<{ status: string }>('PUT', `/timekeeping/roster/${id}`, body)
 }

 async tkLockRoster(id: string) {
 return this.request<{ status: string }>('POST', `/timekeeping/roster/${id}/lock`)
 }

 async tkUnlockRoster(id: string) {
 return this.request<{ status: string }>('POST', `/timekeeping/roster/${id}/unlock`)
 }

 async tkUpdateAssignments(id: string, body: unknown) {
 return this.request<{ data: unknown[] }>('PUT', `/timekeeping/roster/${id}/assignments`, body)
 }

 // ── Timekeeping: Absences ──

 async tkListAbsences(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[] }>('GET', `/timekeeping/absences?${qs}`)
 }

 async tkCreateAbsence(body: unknown) {
 return this.request<{ data: unknown }>('POST', '/timekeeping/absences', body)
 }

 async tkApproveAbsence(id: string, approve: boolean) {
 return this.request<{ status: string }>('POST', `/timekeeping/absences/${id}/approve`, { approve })
 }

 async tkDeleteAbsence(id: string) {
 return this.request<{ status: string }>('DELETE', `/timekeeping/absences/${id}`)
 }

 // ── Timekeeping: Pay Codes ──

 async tkListPayCodes(orgSlug: string) {
 return this.request<{ data: unknown[] }>(
 'GET', `/timekeeping/pay-codes?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async tkCreatePayCode(body: unknown) {
 return this.request<{ data: unknown }>('POST', '/timekeeping/pay-codes', body)
 }

 async tkUpdatePayCode(id: string, body: unknown) {
 return this.request<{ data: unknown }>('PUT', `/timekeeping/pay-codes/${id}`, body)
 }

 // ── Timekeeping: Time Entries ──

 async tkListTimeEntries(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[], total: number, page: number }>(
 'GET', `/timekeeping/time-entries?${qs}`,
 )
 }

 async tkCreateTimeEntry(body: unknown) {
 return this.request<{ data: unknown }>('POST', '/timekeeping/time-entries', body)
 }

 async tkUpdateTimeEntry(id: string, body: unknown) {
 return this.request<{ data: unknown }>('PUT', `/timekeeping/time-entries/${id}`, body)
 }

 async tkDeleteTimeEntry(id: string) {
 return this.request<{ status: string }>('DELETE', `/timekeeping/time-entries/${id}`)
 }

 async tkSubmitTimeEntry(id: string) {
 return this.request<{ status: string }>('POST', `/timekeeping/time-entries/${id}/submit`)
 }

 async tkBatchCreateTimeEntries(body: unknown) {
 return this.request<{ data: unknown[], count: number }>('POST', '/timekeeping/time-entries/batch', body)
 }

 // ── Timekeeping: Periods & Timecards ──

 async tkListPeriods(orgSlug: string) {
 return this.request<{ data: unknown[] }>(
 'GET', `/timekeeping/periods?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async tkCreatePeriod(body: unknown) {
 return this.request<{ data: unknown }>('POST', '/timekeeping/periods', body)
 }

 async tkClosePeriod(id: string) {
 return this.request<{ status: string }>('POST', `/timekeeping/periods/${id}/close`)
 }

 async tkCertifyTimecard(body: unknown) {
 return this.request<{ status: string }>('POST', '/timekeeping/certify', body)
 }

 async tkPendingApprovals(orgSlug: string) {
 return this.request<{ data: unknown[] }>(
 'GET', `/timekeeping/approvals/pending?organization_id=${encodeURIComponent(orgSlug)}`,
 )
 }

 async tkDecideTimecard(body: unknown) {
 return this.request<{ status: string }>('POST', '/timekeeping/approvals/decide', body)
 }

 // ── Timekeeping: Validation ──

 async tkValidate(body: unknown) {
 return this.request<{ total_flags: number, by_type: Record<string, number> }>(
 'POST', '/timekeeping/validate', body,
 )
 }

 async tkListFlags(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[] }>('GET', `/timekeeping/flags?${qs}`)
 }

 async tkResolveFlag(id: string, body?: unknown) {
 return this.request<{ status: string }>('POST', `/timekeeping/flags/${id}/resolve`, body)
 }

 // ── Timekeeping: Reports ──

 async tkHoursReport(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[] }>('GET', `/timekeeping/reports/hours?${qs}`)
 }

 async tkOvertimeReport(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[] }>('GET', `/timekeeping/reports/overtime?${qs}`)
 }

 async tkFlsaReport(orgSlug: string, cycleStart: string) {
 return this.request<{ data: unknown }>(
 'GET', `/timekeeping/reports/flsa?organization_id=${encodeURIComponent(orgSlug)}&cycle_start=${cycleStart}`,
 )
 }

 async tkStaffingReport(orgSlug: string, date: string) {
 return this.request<{ data: unknown }>(
 'GET', `/timekeeping/reports/staffing?organization_id=${encodeURIComponent(orgSlug)}&date=${date}`,
 )
 }

 async tkPayrollExport(orgSlug: string, periodId: string) {
 return this.request<{ data: unknown[] }>(
 'GET', `/timekeeping/reports/payroll-export?organization_id=${encodeURIComponent(orgSlug)}&period_id=${periodId}`,
 )
 }

 // ── Timekeeping: Leave Balances ──

 async tkListLeaveBalances(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[] }>('GET', `/timekeeping/leave-balances?${qs}`)
 }

 async tkAdjustLeaveBalance(body: unknown) {
 return this.request<{ data: unknown }>('PUT', '/timekeeping/leave-balances', body)
 }

 // ── Timekeeping: Audit Trail ──

 async tkAuditTrail(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ data: unknown[], total: number, page: number, per_page: number }>(
 'GET', `/timekeeping/audit?${qs}`,
 )
 }

 async tkAuditSummary(orgSlug: string, params?: Record<string, string>) {
 const qs = new URLSearchParams({ organization_id: orgSlug, ...params })
 return this.request<{ total_events: number, breakdown: unknown[] }>(
 'GET', `/timekeeping/audit/summary?${qs}`,
 )
 }

 // ── Knowledge Learning ──

 async recordSequenceFeedback(id: string, outcome: 'accepted' | 'modified' | 'rejected') {
 return this.request<{ data: unknown }>(
 'POST', `/knowledge/sequences/${id}/feedback`, { outcome },
 )
 }

 async learnFromGoal(body: { organization_id: string; goal_id: string; goal_title: string; task_category: string; tasks: { title: string; description?: string; status?: string; priority?: string; sequence_order: number }[] }) {
 return this.request<{ data: unknown }>(
 'POST', '/knowledge/learn-from-goal', body,
 )
 }

 // ── ACL: Document ACLs ──

 async listDocumentAcls(documentId: string, orgSlug: string) {
 return this.request<{ data: { id: string; document_id: string; grant_type: string; grant_id: string; grant_name: string | null; action: string; created_at: string }[] }>(
 'GET', `/documents/${documentId}/acls?organization_id=${orgSlug}`,
 )
 }

 async createDocumentAcl(documentId: string, body: { organization_id: string; grant_type: string; grant_id: string; action?: string }) {
 return this.request<{ data: unknown }>('POST', `/documents/${documentId}/acls`, body)
 }

 async deleteDocumentAcl(documentId: string, aclId: string, orgSlug: string) {
 return this.request<{ status: string }>('DELETE', `/documents/${documentId}/acls/${aclId}?organization_id=${orgSlug}`)
 }

 async clearDocumentAcls(documentId: string, orgSlug: string) {
 return this.request<{ status: string; removed: number }>('DELETE', `/documents/${documentId}/acls/clear?organization_id=${orgSlug}`)
 }

 // ── ACL: Groups ──

 async listGroups(orgSlug: string) {
 return this.request<{ data: { id: string; name: string; description: string | null; organization_id: string; member_count: number; created_at: string }[] }>(
 'GET', `/groups?organization_id=${orgSlug}`,
 )
 }

 async createGroup(body: { organization_id: string; name: string; description?: string }) {
 return this.request<{ data: unknown }>('POST', '/groups', body)
 }

 async updateGroup(id: string, body: { name?: string; description?: string }) {
 return this.request<{ status: string }>('PUT', `/groups/${id}`, body)
 }

 async deleteGroup(id: string, orgSlug: string) {
 return this.request<{ status: string }>('DELETE', `/groups/${id}?organization_id=${orgSlug}`)
 }

 async listGroupMembers(groupId: string, orgSlug: string) {
 return this.request<{ data: { user_id: string; first_name: string; last_name: string; email: string | null; title: string | null }[] }>(
 'GET', `/groups/${groupId}/members?organization_id=${orgSlug}`,
 )
 }

 async addGroupMember(groupId: string, body: { organization_id: string; user_id: string }) {
 return this.request<{ status: string }>('POST', `/groups/${groupId}/members`, body)
 }

 async removeGroupMember(groupId: string, userId: string, orgSlug: string) {
 return this.request<{ status: string }>('DELETE', `/groups/${groupId}/members/${userId}?organization_id=${orgSlug}`)
 }

 async listGroupPermissions(groupId: string, orgSlug: string) {
 return this.request<{ data: { id: string; group_id: string; feature: string; action: string }[] }>(
 'GET', `/groups/${groupId}/permissions?organization_id=${orgSlug}`,
 )
 }

 async addGroupPermission(groupId: string, body: { organization_id: string; feature: string; action: string }) {
 return this.request<{ data: unknown }>('POST', `/groups/${groupId}/permissions`, body)
 }

 async removeGroupPermission(groupId: string, permId: string, orgSlug: string) {
 return this.request<{ status: string }>('DELETE', `/groups/${groupId}/permissions/${permId}?organization_id=${orgSlug}`)
 }

 // ── ACL: Permissions ──

 async myPermissions(orgSlug: string) {
 return this.request<{ user_id: string; is_admin: boolean; features: Record<string, string[]> }>(
 'GET', `/permissions/me?organization_id=${orgSlug}`,
 )
 }

 async invalidateCache() {
 return this.request<{ status: string }>('POST', '/admin/cache/invalidate')
 }
}
