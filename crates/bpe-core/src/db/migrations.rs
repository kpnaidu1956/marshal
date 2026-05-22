use crate::error::BpeError;

/// Run all BPE schema migrations.
///
/// Creates the `bpe` schema and all tables if they don't exist.
/// Safe to call on every startup (uses IF NOT EXISTS).
pub async fn run_migrations(client: &deadpool_postgres::Client) -> Result<(), BpeError> {
    tracing::info!("Running BPE schema migrations...");

    client.batch_execute(MIGRATION_SQL).await.map_err(|e| {
        BpeError::Database(format!("Migration failed: {e}"))
    })?;

    // Create audit event partition for current month
    create_audit_partition(client).await?;
    // Create entity interactions partition for current quarter
    create_interactions_partition(client).await?;

    // Run timekeeping schema migrations
    crate::timekeeping::migration::run_timekeeping_migrations(client).await?;

    tracing::info!("BPE schema migrations complete");
    Ok(())
}

async fn create_audit_partition(client: &deadpool_postgres::Client) -> Result<(), BpeError> {
    let now = chrono::Utc::now();
    let year = now.format("%Y");
    let month = now.format("%m");
    let next = now + chrono::Months::new(1);
    let next_year = next.format("%Y");
    let next_month = next.format("%m");

    let partition_name = format!("bpe.audit_events_{year}_{month}");
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {partition_name} PARTITION OF bpe.audit_events \
         FOR VALUES FROM ('{year}-{month}-01') TO ('{next_year}-{next_month}-01')"
    );

    client.batch_execute(&sql).await.map_err(|e| {
        BpeError::Database(format!("Audit partition creation failed: {e}"))
    })?;

    // Also create next month's partition
    let next2 = next + chrono::Months::new(1);
    let next2_year = next2.format("%Y");
    let next2_month = next2.format("%m");
    let next_partition = format!("bpe.audit_events_{next_year}_{next_month}");
    let sql2 = format!(
        "CREATE TABLE IF NOT EXISTS {next_partition} PARTITION OF bpe.audit_events \
         FOR VALUES FROM ('{next_year}-{next_month}-01') TO ('{next2_year}-{next2_month}-01')"
    );
    let _ = client.batch_execute(&sql2).await;

    Ok(())
}

async fn create_interactions_partition(client: &deadpool_postgres::Client) -> Result<(), BpeError> {
    let now = chrono::Utc::now();
    let quarter_start = quarter_start(now);
    let quarter_end = quarter_start + chrono::Months::new(3);

    let qs = quarter_start.format("%Y_%m");
    let partition_name = format!("bpe.entity_interactions_{qs}");
    let from_date = quarter_start.format("%Y-%m-%d");
    let to_date = quarter_end.format("%Y-%m-%d");

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {partition_name} PARTITION OF bpe.entity_interactions \
         FOR VALUES FROM ('{from_date}') TO ('{to_date}')"
    );
    let _ = client.batch_execute(&sql).await;

    Ok(())
}

fn quarter_start(dt: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, TimeZone};
    let month = ((dt.month() - 1) / 3) * 3 + 1;
    chrono::Utc
        .with_ymd_and_hms(dt.year(), month, 1, 0, 0, 0)
        .unwrap()
}

const MIGRATION_SQL: &str = r#"
CREATE SCHEMA IF NOT EXISTS bpe;

-- 1. Entity Types
CREATE TABLE IF NOT EXISTS bpe.entity_types (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(100) NOT NULL,
    display_name VARCHAR(200) NOT NULL,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT false,
    icon VARCHAR(50),
    color VARCHAR(7),
    core_fields JSONB NOT NULL DEFAULT '[]'::jsonb,
    custom_fields JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);
CREATE INDEX IF NOT EXISTS idx_entity_types_org ON bpe.entity_types(organization_id);

-- 2. Entities
CREATE TABLE IF NOT EXISTS bpe.entities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    entity_type_id UUID NOT NULL REFERENCES bpe.entity_types(id),
    linked_user_id UUID REFERENCES api.users(id),
    display_name VARCHAR(500) NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES api.users(id)
);
CREATE INDEX IF NOT EXISTS idx_entities_org ON bpe.entities(organization_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON bpe.entities(entity_type_id);
CREATE INDEX IF NOT EXISTS idx_entities_linked_user ON bpe.entities(linked_user_id) WHERE linked_user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_entities_status ON bpe.entities(organization_id, status);

-- 3. Entity Relationships
CREATE TABLE IF NOT EXISTS bpe.entity_relationships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    source_entity_id UUID NOT NULL REFERENCES bpe.entities(id) ON DELETE CASCADE,
    target_entity_id UUID NOT NULL REFERENCES bpe.entities(id) ON DELETE CASCADE,
    relationship_type VARCHAR(100) NOT NULL,
    metadata JSONB DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT no_self_relationship CHECK (source_entity_id != target_entity_id)
);
CREATE INDEX IF NOT EXISTS idx_entity_rels_org ON bpe.entity_relationships(organization_id);
CREATE INDEX IF NOT EXISTS idx_entity_rels_source ON bpe.entity_relationships(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_rels_target ON bpe.entity_relationships(target_entity_id);

-- 4. Entity Interactions (partitioned — PK includes created_at for partitioning)
CREATE TABLE IF NOT EXISTS bpe.entity_interactions (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    entity_id UUID NOT NULL,
    interaction_type VARCHAR(100) NOT NULL,
    source_type VARCHAR(100),
    source_id UUID,
    performed_by UUID,
    summary TEXT NOT NULL,
    details JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- 5. Approval Rules (must precede workflow_steps FK)
CREATE TABLE IF NOT EXISTS bpe.approval_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(300) NOT NULL,
    description TEXT,
    conditions JSONB NOT NULL DEFAULT '{"conditions":[],"logic":"all"}'::jsonb,
    approval_type VARCHAR(50) NOT NULL DEFAULT 'single',
    approver_user_ids UUID[] NOT NULL DEFAULT '{}',
    required_approvals INTEGER NOT NULL DEFAULT 1,
    timeout_minutes INTEGER NOT NULL DEFAULT 0,
    escalation_user_id UUID REFERENCES api.users(id),
    auto_approve_on_timeout BOOLEAN NOT NULL DEFAULT false,
    allow_delegation BOOLEAN NOT NULL DEFAULT true,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_approval_rules_org ON bpe.approval_rules(organization_id);

-- 6. Workflow Definitions
CREATE TABLE IF NOT EXISTS bpe.workflow_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(300) NOT NULL,
    description TEXT,
    category VARCHAR(100) NOT NULL DEFAULT 'custom',
    applicable_entity_types UUID[] DEFAULT '{}',
    step_templates JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_learned BOOLEAN NOT NULL DEFAULT false,
    source VARCHAR(50) NOT NULL DEFAULT 'manual',
    version INTEGER NOT NULL DEFAULT 1,
    is_active BOOLEAN NOT NULL DEFAULT true,
    times_used INTEGER NOT NULL DEFAULT 0,
    avg_completion_minutes DOUBLE PRECISION,
    success_rate DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES api.users(id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_defs_org ON bpe.workflow_definitions(organization_id);
CREATE INDEX IF NOT EXISTS idx_workflow_defs_category ON bpe.workflow_definitions(organization_id, category);

-- 7. Workflow Executions
CREATE TABLE IF NOT EXISTS bpe.workflow_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    definition_id UUID REFERENCES bpe.workflow_definitions(id),
    title VARCHAR(500) NOT NULL,
    description TEXT,
    original_prompt TEXT,
    target_entity_id UUID REFERENCES bpe.entities(id),
    linked_task_id UUID,
    linked_goal_id UUID,
    status VARCHAR(50) NOT NULL DEFAULT 'draft',
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    initiated_by UUID NOT NULL REFERENCES api.users(id),
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_wf_exec_org ON bpe.workflow_executions(organization_id);
CREATE INDEX IF NOT EXISTS idx_wf_exec_status ON bpe.workflow_executions(organization_id, status);
CREATE INDEX IF NOT EXISTS idx_wf_exec_initiator ON bpe.workflow_executions(initiated_by);

-- 8. Workflow Steps
CREATE TABLE IF NOT EXISTS bpe.workflow_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    execution_id UUID NOT NULL REFERENCES bpe.workflow_executions(id) ON DELETE CASCADE,
    step_order INTEGER NOT NULL,
    name VARCHAR(300) NOT NULL,
    description TEXT,
    step_type VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    dependencies INTEGER[] DEFAULT '{}',
    estimated_duration_minutes INTEGER,
    actual_duration_minutes INTEGER,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    integration_type VARCHAR(100),
    integration_config JSONB DEFAULT '{}'::jsonb,
    integration_result JSONB,
    approval_rule_id UUID REFERENCES bpe.approval_rules(id),
    approval_request_id UUID,
    assigned_to UUID REFERENCES api.users(id),
    input_data JSONB DEFAULT '{}'::jsonb,
    output_data JSONB DEFAULT '{}'::jsonb,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(execution_id, step_order)
);
CREATE INDEX IF NOT EXISTS idx_wf_steps_exec ON bpe.workflow_steps(execution_id);
CREATE INDEX IF NOT EXISTS idx_wf_steps_status ON bpe.workflow_steps(execution_id, status);

-- 9. Approval Requests
CREATE TABLE IF NOT EXISTS bpe.approval_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    rule_id UUID NOT NULL REFERENCES bpe.approval_rules(id),
    workflow_execution_id UUID REFERENCES bpe.workflow_executions(id),
    workflow_step_id UUID REFERENCES bpe.workflow_steps(id),
    title VARCHAR(500) NOT NULL,
    description TEXT,
    context_data JSONB DEFAULT '{}'::jsonb,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    requested_by UUID NOT NULL REFERENCES api.users(id),
    current_approver_index INTEGER NOT NULL DEFAULT 0,
    deadline_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    resolution_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_approval_reqs_org ON bpe.approval_requests(organization_id);
CREATE INDEX IF NOT EXISTS idx_approval_reqs_status ON bpe.approval_requests(organization_id, status);

-- 10. Approval Decisions
CREATE TABLE IF NOT EXISTS bpe.approval_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    request_id UUID NOT NULL REFERENCES bpe.approval_requests(id) ON DELETE CASCADE,
    decided_by UUID NOT NULL REFERENCES api.users(id),
    delegated_from UUID REFERENCES api.users(id),
    decision VARCHAR(50) NOT NULL,
    notes TEXT,
    decided_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_approval_decisions_req ON bpe.approval_decisions(request_id);

-- 11. Integration Credentials
CREATE TABLE IF NOT EXISTS bpe.integration_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    integration_type VARCHAR(100) NOT NULL,
    name VARCHAR(300) NOT NULL,
    encrypted_credentials BYTEA NOT NULL,
    encryption_nonce BYTEA NOT NULL,
    last_test_at TIMESTAMPTZ,
    last_test_success BOOLEAN,
    last_test_error TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES api.users(id),
    UNIQUE(organization_id, integration_type, name)
);
CREATE INDEX IF NOT EXISTS idx_int_creds_org ON bpe.integration_credentials(organization_id);

-- 12. Audit Events (partitioned by month)
CREATE TABLE IF NOT EXISTS bpe.audit_events (
    id BIGSERIAL,
    organization_id UUID NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    resource_type VARCHAR(100) NOT NULL,
    resource_id UUID NOT NULL,
    actor_user_id UUID,
    actor_type VARCHAR(50) NOT NULL DEFAULT 'user',
    before_state JSONB,
    after_state JSONB,
    metadata JSONB DEFAULT '{}'::jsonb,
    ip_address INET,
    is_reversed BOOLEAN NOT NULL DEFAULT false,
    reversed_by_event_id BIGINT,
    reversal_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- 13. Learned Sequences
CREATE TABLE IF NOT EXISTS bpe.learned_sequences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    task_category VARCHAR(200) NOT NULL,
    entity_type_names VARCHAR(100)[] DEFAULT '{}',
    steps JSONB NOT NULL,
    source_execution_id UUID REFERENCES bpe.workflow_executions(id),
    times_suggested INTEGER NOT NULL DEFAULT 0,
    times_accepted INTEGER NOT NULL DEFAULT 0,
    times_modified INTEGER NOT NULL DEFAULT 0,
    times_rejected INTEGER NOT NULL DEFAULT 0,
    avg_completion_minutes DOUBLE PRECISION,
    embedding_text TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    superseded_by UUID REFERENCES bpe.learned_sequences(id),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_learned_seq_org ON bpe.learned_sequences(organization_id);
CREATE INDEX IF NOT EXISTS idx_learned_seq_category ON bpe.learned_sequences(organization_id, task_category);

-- 14. Report Templates
CREATE TABLE IF NOT EXISTS bpe.report_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID,
    name VARCHAR(300) NOT NULL,
    description TEXT,
    category VARCHAR(100) NOT NULL,
    sql_template TEXT NOT NULL,
    parameters JSONB NOT NULL DEFAULT '[]'::jsonb,
    columns JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 15. Notifications
CREATE TABLE IF NOT EXISTS bpe.notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    recipient_user_id UUID NOT NULL REFERENCES api.users(id),
    source_type VARCHAR(100) NOT NULL,
    source_id UUID NOT NULL,
    title VARCHAR(500) NOT NULL,
    body TEXT,
    channel VARCHAR(50) NOT NULL DEFAULT 'in_app',
    is_read BOOLEAN NOT NULL DEFAULT false,
    read_at TIMESTAMPTZ,
    email_sent BOOLEAN NOT NULL DEFAULT false,
    email_sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_notifications_recipient ON bpe.notifications(recipient_user_id, is_read);
CREATE INDEX IF NOT EXISTS idx_notifications_org ON bpe.notifications(organization_id);
"#;
