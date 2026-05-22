// BPE (Business Process Engine) TypeScript models

// --- Entity Types ---
export interface BpeEntityType {
 id: string
 organization_id: string
 name: string
 display_name: string
 description: string | null
 is_system: boolean
 icon: string | null
 color: string | null
 core_fields: unknown[]
 custom_fields: unknown[]
 created_at: string
 updated_at: string
}

// --- Entities ---
export interface BpeEntity {
 id: string
 organization_id: string
 entity_type_id: string
 linked_user_id: string | null
 display_name: string
 attributes: Record<string, unknown>
 status: string
 created_at: string
 updated_at: string
 created_by: string | null
 entity_type_name: string | null
}

// --- Step Template (for workflow definition steps) ---
export interface StepTemplate {
 name: string
 description?: string | null
 step_type: string
 dependencies: number[]
 estimated_duration_minutes?: number | null
 integration_type?: string | null
 integration_config?: Record<string, unknown> | null
 assigned_role?: string | null
 execution_rule?: 'all' | 'any' | null
 condition?: string | null
 is_terminal?: boolean
}

// --- Workflow Definitions ---
export interface WorkflowDefinition {
 id: string
 organization_id: string
 name: string
 description: string | null
 category: string
 step_templates: StepTemplate[]
 source: string
 version: number
 times_used: number
 avg_completion_minutes: number | null
 success_rate: number | null
 is_active: boolean
 created_by: string | null
 created_at: string
 updated_at: string
}

// --- Workflow Executions ---
export interface WorkflowExecution {
 id: string
 organization_id: string
 definition_id: string
 entity_id: string | null
 status: string
 started_at: string | null
 completed_at: string | null
 initiated_by: string | null
 created_at: string
}

// --- Workflow Steps ---
export interface WorkflowStep {
 id: string
 organization_id: string
 execution_id: string
 step_order: number
 name: string
 description: string | null
 step_type: string
 status: string
 dependencies: number[]
 estimated_duration_minutes: number | null
 actual_duration_minutes: number | null
 started_at: string | null
 completed_at: string | null
 integration_type: string | null
 integration_config: Record<string, unknown> | null
 integration_result: Record<string, unknown> | null
 assigned_to: string | null
 output_data: Record<string, unknown> | null
 error_message: string | null
 retry_count: number
 max_retries: number
 created_at: string
 updated_at: string
}

// --- Approval Rules ---
export interface ApprovalRule {
 id: string
 organization_id: string
 name: string
 description: string | null
 resource_type: string
 condition_expression: string | null
 approval_type: string
 required_approvers: string[]
 min_approvals: number
 is_active: boolean
 created_at: string
 updated_at: string
}

// --- Approval Requests ---
export interface ApprovalRequest {
 id: string
 organization_id: string
 rule_id: string
 resource_type: string
 resource_id: string
 requested_by: string
 status: string
 created_at: string
 resolved_at: string | null
}

// --- Approval Decisions ---
export interface ApprovalDecision {
 id: string
 request_id: string
 decided_by: string
 decision: string
 comment: string | null
 created_at: string
}

// --- Integration Credentials ---
export interface IntegrationCredential {
 id: string
 organization_id: string
 integration_type: string
 name: string
 is_active: boolean
 last_tested_at: string | null
 last_test_success: boolean | null
 created_at: string
 updated_at: string
}

export interface IntegrationType {
 name: string
 display_name: string
 description: string
 actions: string[]
 credential_fields: string[]
}

// --- Knowledge / Learned Sequences ---
export interface LearnedSequence {
 id: string
 organization_id: string
 name: string
 description: string | null
 source_execution_id: string
 steps: unknown[]
 times_suggested: number
 times_accepted: number
 acceptance_rate: number | null
 is_active: boolean
 created_at: string
}

// --- Report Templates ---
export interface ReportTemplate {
 id: string
 organization_id: string | null
 name: string
 description: string | null
 category: string
 sql_template: string
 parameters: unknown[]
 columns: unknown[]
 is_active: boolean
 created_at: string
 updated_at: string
}

export interface ReportResult {
 template_id: string
 template_name: string
 columns: unknown[]
 rows: Record<string, unknown>[]
 row_count: number
 generated_at: string
}

// --- Notifications ---
export interface BpeNotification {
 id: string
 organization_id: string
 recipient_user_id: string
 source_type: string
 source_id: string
 title: string
 body: string | null
 channel: string
 is_read: boolean
 read_at: string | null
 email_sent: boolean
 email_sent_at: string | null
 created_at: string
}

// --- Dashboard ---
export interface BpeDashboard {
 entities: number
 workflow_definitions: number
 workflow_executions: {
 total: number
 active: number
 completed: number
 }
 pending_approvals: number
 audit_events_24h: number
 learned_sequences: number
 generated_at: string
}

// --- Workflow Performance ---
export interface WorkflowPerformanceItem {
 name: string
 category: string
 execution_count: number
 completed: number
 failed: number
 success_rate: number
 avg_duration_minutes: number | null
}

// --- Timeline ---
export interface TimelineEvent {
 timestamp: string
 event_type: string
 description: string
 actor: string | null
}

// --- Audit Events ---
export interface AuditEvent {
 id: string
 organization_id: string
 actor_id: string
 action: string
 resource_type: string
 resource_id: string
 old_value: unknown | null
 new_value: unknown | null
 created_at: string
}
