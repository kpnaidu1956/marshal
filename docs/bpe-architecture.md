# Business Process Engine (BPE) — Architecture Document

Version: 1.0.0
Date: 2026-04-01
Status: Design Phase

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Crate Structure](#2-crate-structure)
3. [Database Schema](#3-database-schema)
4. [API Design](#4-api-design)
5. [Module Architecture](#5-module-architecture)
6. [Workflow Engine Design](#6-workflow-engine-design)
7. [Entity System Design](#7-entity-system-design)
8. [Approval Engine Design](#8-approval-engine-design)
9. [Integration Framework](#9-integration-framework)
10. [Audit System Design](#10-audit-system-design)
11. [Knowledge Learning Loop](#11-knowledge-learning-loop)
12. [Frontend Components](#12-frontend-components)
13. [Data Flow Diagrams](#13-data-flow-diagrams)
14. [Security Considerations](#14-security-considerations)
15. [Migration Strategy](#15-migration-strategy)
16. [Build Order](#16-build-order)

---

## 1. System Overview

### 1.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Client Layer                                   │
│  ┌──────────────────────┐  ┌──────────────────┐  ┌───────────────────────┐ │
│  │  marshal-ui-react    │  │  MCP Clients      │  │  External Webhooks   │ │
│  │  (React SPA)         │  │  (goalrag-mcp)    │  │  (Adapters)          │ │
│  └──────────┬───────────┘  └────────┬─────────┘  └──────────┬────────────┘ │
└─────────────┼──────────────────────┼────────────────────────┼──────────────┘
              │                      │                        │
              ▼                      ▼                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Reverse Proxy (nginx)                             │
│   /api/*  → goal-rag:8080    /bpe/*  → bpe-server:8090                     │
│   /marshal/* → static files  /postgrest/* → PostgREST:3000                 │
└──────────────────┬──────────────────────────┬───────────────────────────────┘
                   │                          │
    ┌──────────────▼──────────┐    ┌──────────▼──────────────────┐
    │   goal-rag (port 8080)  │    │   bpe-server (port 8090)    │
    │                         │    │                              │
    │  - Document ingestion   │    │  - Workflow engine           │
    │  - RAG query            │    │  - Entity management         │
    │  - Knowledge base       │    │  - Approval engine           │
    │  - Analytics            │    │  - Integration adapters      │
    │  - LLM tools            │    │  - Audit trail               │
    │  - Auth (JWT issuer)    │    │  - Reporting                 │
    │  - File storage         │    │  - Knowledge learning        │
    └────────────┬────────────┘    └──────────┬──────────────────┘
                 │                             │
                 │    ┌────────────────────┐   │
                 │    │   Gemini LLM       │   │
                 │    │   (Vertex AI)      │◄──┤
                 │    └────────────────────┘   │
                 │                             │
                 ▼                             ▼
    ┌──────────────────────────────────────────────────────────┐
    │                  PostgreSQL (goalrag DB)                  │
    │                                                          │
    │  api.*              bpe.*               public.*         │
    │  (existing tables)  (new BPE tables)    (rag_chunks,     │
    │  users, tasks,      workflows, steps,    entity_embeds)  │
    │  goals, orgs...     entities, approvals                  │
    └──────────────────────────────────────────────────────────┘
```

### 1.2 Design Principles

- **Shared DB, Separate Binary**: BPE runs as `bpe-server` on port 8090 alongside `goal-rag` on 8080. Both connect to the same `goalrag` PostgreSQL database. BPE uses its own `bpe` schema.
- **Shared Auth**: BPE validates the same JWT tokens issued by goal-rag's `/api/auth/login`. No separate auth service.
- **Organization Isolation**: Every BPE table includes `organization_id UUID NOT NULL`. All queries filter by org.
- **RAG Integration**: BPE calls goal-rag's `/api/v2/query` endpoint internally for knowledge-base-powered step generation. No direct access to RAG internals.
- **Gemini Direct**: BPE has its own Gemini client for task decomposition prompts (reuses the same GCP auth pattern from goal-rag).
- **Event Sourced Audit**: All state changes recorded as immutable events. Current state is derived from event log for critical paths.

### 1.3 Technology Stack

| Layer | Technology | Notes |
|---|---|---|
| Language | Rust 2021 edition | Same workspace |
| Web framework | Axum 0.7 | Same version as goal-rag |
| Database | PostgreSQL 15+ (pgvector) | Same cluster, `bpe` schema |
| Connection pool | deadpool-postgres 0.14 | Same pattern as goal-rag |
| Auth | JWT HMAC-SHA256 | Validates tokens from goal-rag |
| LLM | Gemini 2.5 Pro (Vertex AI) | Task decomposition, reporting |
| Serialization | serde + serde_json | Standard |
| Encryption | ring 0.17 | Credential encryption at rest |
| Frontend | React + TypeScript | marshal-ui-react additions |
| HTTP client | reqwest 0.12 | RAG queries, external integrations |

---

## 2. Crate Structure

### 2.1 Workspace Layout

```
marshal/
├── Cargo.toml                          # Add bpe-server and bpe-core
├── crates/
│   ├── goal-rag/                       # Existing RAG service (unchanged)
│   ├── goalrag-mcp/                    # Existing MCP proxy (unchanged)
│   ├── marshal-ui-react/               # React frontend (BPE pages added)
│   │   └── src/
│   │       └── pages/
│   │           └── processes/          # NEW: BPE frontend pages
│   ├── bpe-core/                       # NEW: BPE domain logic library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs               # BPE configuration
│   │       ├── error.rs                # Error types
│   │       ├── auth.rs                 # JWT validation (no issuing)
│   │       ├── db/
│   │       │   ├── mod.rs
│   │       │   ├── pool.rs             # PgPool (reuse pattern from goal-rag)
│   │       │   └── migrations.rs       # Schema auto-migration
│   │       ├── workflow/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # Workflow execution engine
│   │       │   ├── state_machine.rs    # Step state transitions
│   │       │   ├── decomposer.rs       # LLM-powered goal decomposition
│   │       │   ├── scheduler.rs        # Step scheduling & dependency resolution
│   │       │   └── models.rs           # Workflow/Step/Execution types
│   │       ├── entity/
│   │       │   ├── mod.rs
│   │       │   ├── registry.rs         # Entity type registry
│   │       │   ├── attributes.rs       # Attribute definitions & validation
│   │       │   ├── relationships.rs    # Entity relationship management
│   │       │   ├── interactions.rs     # Interaction tracking
│   │       │   └── models.rs           # Entity/EntityType/Relationship types
│   │       ├── approval/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # Approval rule evaluation
│   │       │   ├── rules.rs            # Rule DSL and condition evaluation
│   │       │   ├── escalation.rs       # Escalation & timeout logic
│   │       │   └── models.rs           # ApprovalRule/Request/Decision types
│   │       ├── integration/
│   │       │   ├── mod.rs
│   │       │   ├── registry.rs         # Adapter registry
│   │       │   ├── credentials.rs      # Encrypted credential storage
│   │       │   ├── adapter.rs          # Adapter trait definition
│   │       │   ├── adapters/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── email.rs        # SMTP adapter
│   │       │   │   ├── webhook.rs      # Generic REST/webhook adapter
│   │       │   │   ├── xero.rs         # Xero accounting adapter
│   │       │   │   ├── docusign.rs     # DocuSign adapter
│   │       │   │   └── quickbooks.rs   # QuickBooks adapter
│   │       │   └── models.rs           # IntegrationConfig/Credential types
│   │       ├── audit/
│   │       │   ├── mod.rs
│   │       │   ├── logger.rs           # Event recording
│   │       │   ├── reversal.rs         # Compensation/reversal logic
│   │       │   ├── query.rs            # Audit trail queries
│   │       │   └── models.rs           # AuditEvent types
│   │       ├── knowledge/
│   │       │   ├── mod.rs
│   │       │   ├── learner.rs          # Sequence learning from executions
│   │       │   ├── suggester.rs        # Suggest learned sequences
│   │       │   └── models.rs           # LearnedSequence types
│   │       ├── reporting/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # NL-to-SQL + LLM formatting
│   │       │   ├── templates.rs        # Pre-built report definitions
│   │       │   ├── export.rs           # CSV/Excel/PDF export
│   │       │   └── models.rs           # Report types
│   │       └── llm/
│   │           ├── mod.rs
│   │           ├── gemini.rs           # Gemini client (decomposition, reporting)
│   │           └── prompts.rs          # Prompt templates
│   └── bpe-server/                     # NEW: BPE HTTP server binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # Server entrypoint
│           ├── state.rs                # AppState
│           └── routes/
│               ├── mod.rs              # Route tree
│               ├── workflows.rs        # Workflow CRUD + execution
│               ├── entities.rs         # Entity CRUD
│               ├── entity_types.rs     # Entity type management
│               ├── approvals.rs        # Approval endpoints
│               ├── integrations.rs     # Integration management
│               ├── audit.rs            # Audit trail queries
│               ├── reports.rs          # Reporting endpoints
│               └── health.rs           # Health check
```

### 2.2 Cargo.toml Additions

```toml
# Root Cargo.toml — add to [workspace] members:
"crates/bpe-core",
"crates/bpe-server",

# crates/bpe-core/Cargo.toml
[package]
name = "bpe-core"
version.workspace = true
edition.workspace = true

[dependencies]
# Database
tokio-postgres = { version = "0.7", features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
deadpool-postgres = "0.14"
postgres-types = { version = "0.2", features = ["derive"] }

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }

# Auth
hmac = "0.12"
sha2 = "0.10"
base64 = "0.22"

# Encryption (credentials at rest)
ring = "0.17"

# HTTP client (for RAG queries, external integrations)
reqwest = { version = "0.12", features = ["json"] }

# Async
tokio = { workspace = true, features = ["full"] }
async-trait = "0.1"
futures = "0.3"

# Utilities
uuid = { workspace = true }
chrono = { version = "0.4", features = ["serde"] }
thiserror = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }

# crates/bpe-server/Cargo.toml
[package]
name = "bpe-server"
version.workspace = true
edition.workspace = true

[dependencies]
bpe-core = { path = "../bpe-core" }
axum = { version = "0.7", features = ["json"] }
tokio = { workspace = true, features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
uuid = { workspace = true }
chrono = { version = "0.4", features = ["serde"] }

[[bin]]
name = "bpe-server"
path = "src/main.rs"
```

---

## 3. Database Schema

All BPE tables live in the `bpe` schema within the existing `goalrag` database. The `api` schema tables (users, organizations, tasks, goals) are referenced via foreign keys.

### 3.1 Schema Creation

```sql
CREATE SCHEMA IF NOT EXISTS bpe;

-- Grant BPE server access to api schema for FK lookups
GRANT USAGE ON SCHEMA api TO postgres;
GRANT SELECT ON ALL TABLES IN SCHEMA api TO postgres;
```

### 3.2 Entity Type System

```sql
-- Pre-defined + custom entity types per organization
CREATE TABLE bpe.entity_types (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(100) NOT NULL,           -- e.g., 'employee', 'supplier', 'customer'
    display_name VARCHAR(200) NOT NULL,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT false,  -- true for pre-defined types
    icon VARCHAR(50),                     -- lucide icon name for UI
    color VARCHAR(7),                     -- hex color for UI
    -- Core fields schema (JSONB array of field definitions)
    -- Each: { "name": "hire_date", "type": "date", "required": true, "label": "Hire Date" }
    core_fields JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Custom fields defined by org (same format)
    custom_fields JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);

CREATE INDEX idx_entity_types_org ON bpe.entity_types(organization_id);

-- Seed system entity types (run per-org on first access)
-- Seeding handled by bpe-core/src/entity/registry.rs
```

### 3.3 Entities

```sql
CREATE TABLE bpe.entities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    entity_type_id UUID NOT NULL REFERENCES bpe.entity_types(id),
    -- Optional link to api.users (for Employee entities)
    linked_user_id UUID REFERENCES api.users(id),
    -- Display name (computed or manual)
    display_name VARCHAR(500) NOT NULL,
    -- All attribute values stored as JSONB
    -- Keys match field names from entity_types.core_fields + custom_fields
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(50) NOT NULL DEFAULT 'active',  -- active, inactive, archived
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES api.users(id)
);

CREATE INDEX idx_entities_org ON bpe.entities(organization_id);
CREATE INDEX idx_entities_type ON bpe.entities(entity_type_id);
CREATE INDEX idx_entities_linked_user ON bpe.entities(linked_user_id) WHERE linked_user_id IS NOT NULL;
CREATE INDEX idx_entities_status ON bpe.entities(organization_id, status);
CREATE INDEX idx_entities_attrs ON bpe.entities USING gin(attributes);
```

### 3.4 Entity Relationships

```sql
CREATE TABLE bpe.entity_relationships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    source_entity_id UUID NOT NULL REFERENCES bpe.entities(id) ON DELETE CASCADE,
    target_entity_id UUID NOT NULL REFERENCES bpe.entities(id) ON DELETE CASCADE,
    relationship_type VARCHAR(100) NOT NULL,  -- 'reports_to', 'manages', 'contracted_by', 'invoices'
    metadata JSONB DEFAULT '{}'::jsonb,       -- e.g., { "start_date": "...", "contract_id": "..." }
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT no_self_relationship CHECK (source_entity_id != target_entity_id)
);

CREATE INDEX idx_entity_rels_org ON bpe.entity_relationships(organization_id);
CREATE INDEX idx_entity_rels_source ON bpe.entity_relationships(source_entity_id);
CREATE INDEX idx_entity_rels_target ON bpe.entity_relationships(target_entity_id);
CREATE INDEX idx_entity_rels_type ON bpe.entity_relationships(organization_id, relationship_type);
```

### 3.5 Entity Interactions

```sql
CREATE TABLE bpe.entity_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    entity_id UUID NOT NULL REFERENCES bpe.entities(id) ON DELETE CASCADE,
    -- What triggered this interaction
    interaction_type VARCHAR(100) NOT NULL,  -- 'workflow_step', 'approval', 'email', 'payment', 'note'
    -- Reference to the source (workflow execution, approval, etc.)
    source_type VARCHAR(100),     -- 'workflow_execution', 'approval_request', 'manual'
    source_id UUID,               -- FK to the relevant table
    -- Who performed the interaction
    performed_by UUID REFERENCES api.users(id),
    -- Details
    summary TEXT NOT NULL,
    details JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
) PARTITION BY RANGE (created_at);

CREATE INDEX idx_entity_interactions_org ON bpe.entity_interactions(organization_id);
CREATE INDEX idx_entity_interactions_entity ON bpe.entity_interactions(entity_id);
CREATE INDEX idx_entity_interactions_type ON bpe.entity_interactions(interaction_type);

-- Partition per quarter
CREATE TABLE bpe.entity_interactions_2026_q2 PARTITION OF bpe.entity_interactions
    FOR VALUES FROM ('2026-04-01') TO ('2026-07-01');
CREATE TABLE bpe.entity_interactions_2026_q3 PARTITION OF bpe.entity_interactions
    FOR VALUES FROM ('2026-07-01') TO ('2026-10-01');
CREATE TABLE bpe.entity_interactions_2026_q4 PARTITION OF bpe.entity_interactions
    FOR VALUES FROM ('2026-10-01') TO ('2027-01-01');
```

### 3.6 Workflow Definitions

```sql
-- A workflow template (reusable process definition)
CREATE TABLE bpe.workflow_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(300) NOT NULL,
    description TEXT,
    -- Category for grouping: 'hr', 'finance', 'procurement', 'legal', 'operations', 'custom'
    category VARCHAR(100) NOT NULL DEFAULT 'custom',
    -- Applicable entity types (empty = any)
    applicable_entity_types UUID[] DEFAULT '{}',
    -- The step template (JSONB array of step definitions)
    -- Each: { "order": 1, "name": "...", "description": "...", "type": "manual|automated|approval|integration",
    --         "estimated_duration_minutes": 30, "dependencies": [0], "integration_type": "email",
    --         "approval_rule_id": null, "config": {} }
    step_templates JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Whether this was learned from execution (vs manually defined)
    is_learned BOOLEAN NOT NULL DEFAULT false,
    -- Source: 'manual', 'llm_generated', 'learned_from_execution'
    source VARCHAR(50) NOT NULL DEFAULT 'manual',
    -- Version tracking
    version INTEGER NOT NULL DEFAULT 1,
    is_active BOOLEAN NOT NULL DEFAULT true,
    -- Stats
    times_used INTEGER NOT NULL DEFAULT 0,
    avg_completion_minutes DOUBLE PRECISION,
    success_rate DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES api.users(id)
);

CREATE INDEX idx_workflow_defs_org ON bpe.workflow_definitions(organization_id);
CREATE INDEX idx_workflow_defs_category ON bpe.workflow_definitions(organization_id, category);
CREATE INDEX idx_workflow_defs_learned ON bpe.workflow_definitions(organization_id, is_learned)
    WHERE is_learned = true;
```

### 3.7 Workflow Executions

```sql
-- A running instance of a workflow
CREATE TABLE bpe.workflow_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    -- Optional: which definition this was based on (null if ad-hoc from LLM)
    definition_id UUID REFERENCES bpe.workflow_definitions(id),
    -- What triggered this workflow
    title VARCHAR(500) NOT NULL,
    description TEXT,
    -- The original goal/task description submitted by the user
    original_prompt TEXT,
    -- Entity this workflow operates on (e.g., the new hire employee entity)
    target_entity_id UUID REFERENCES bpe.entities(id),
    -- Linked goal-rag task/goal (optional)
    linked_task_id UUID,     -- api.tasks.id
    linked_goal_id UUID,     -- api.goals.id
    -- State
    status VARCHAR(50) NOT NULL DEFAULT 'draft',
    -- draft: steps generated, user reviewing
    -- confirmed: user confirmed steps, ready to execute
    -- running: actively executing steps
    -- paused: manually paused or waiting for approval
    -- completed: all steps done
    -- failed: unrecoverable error
    -- cancelled: user cancelled
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    -- Who initiated
    initiated_by UUID NOT NULL REFERENCES api.users(id),
    -- Execution metadata
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_wf_exec_org ON bpe.workflow_executions(organization_id);
CREATE INDEX idx_wf_exec_status ON bpe.workflow_executions(organization_id, status);
CREATE INDEX idx_wf_exec_entity ON bpe.workflow_executions(target_entity_id)
    WHERE target_entity_id IS NOT NULL;
CREATE INDEX idx_wf_exec_initiator ON bpe.workflow_executions(initiated_by);
```

### 3.8 Workflow Steps

```sql
-- Individual steps within a workflow execution
CREATE TABLE bpe.workflow_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    execution_id UUID NOT NULL REFERENCES bpe.workflow_executions(id) ON DELETE CASCADE,
    -- Ordering
    step_order INTEGER NOT NULL,
    -- Step definition
    name VARCHAR(300) NOT NULL,
    description TEXT,
    -- Step type determines execution behavior
    step_type VARCHAR(50) NOT NULL,
    -- 'manual': user marks as done
    -- 'automated': system executes via integration adapter
    -- 'approval': requires approval before proceeding
    -- 'integration': calls external system
    -- 'llm_action': LLM generates and executes (e.g., draft email)
    -- 'sub_workflow': spawns a child workflow
    -- State
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    -- pending: not yet started
    -- ready: dependencies met, can start
    -- in_progress: currently executing
    -- waiting_approval: blocked on approval
    -- waiting_integration: blocked on external system
    -- completed: done
    -- failed: error occurred
    -- skipped: user skipped
    -- Dependencies (step_order values that must complete first)
    dependencies INTEGER[] DEFAULT '{}',
    -- Timing
    estimated_duration_minutes INTEGER,
    actual_duration_minutes INTEGER,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    -- Integration details (if step_type = 'integration' or 'automated')
    integration_type VARCHAR(100),           -- e.g., 'email', 'xero', 'docusign'
    integration_config JSONB DEFAULT '{}'::jsonb,  -- adapter-specific config
    integration_result JSONB,                -- response from external system
    -- Approval details (if step_type = 'approval')
    approval_rule_id UUID REFERENCES bpe.approval_rules(id),
    approval_request_id UUID,  -- FK set after approval request created
    -- Assigned to (for manual steps)
    assigned_to UUID REFERENCES api.users(id),
    -- User-provided input/output
    input_data JSONB DEFAULT '{}'::jsonb,
    output_data JSONB DEFAULT '{}'::jsonb,
    -- Error tracking
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(execution_id, step_order)
);

CREATE INDEX idx_wf_steps_exec ON bpe.workflow_steps(execution_id);
CREATE INDEX idx_wf_steps_status ON bpe.workflow_steps(execution_id, status);
CREATE INDEX idx_wf_steps_assigned ON bpe.workflow_steps(assigned_to) WHERE assigned_to IS NOT NULL;
```

### 3.9 Approval Rules

```sql
CREATE TABLE bpe.approval_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(300) NOT NULL,
    description TEXT,
    -- Rule conditions as structured JSON
    -- { "conditions": [
    --     { "field": "amount", "operator": "gt", "value": 10000 },
    --     { "field": "entity_type", "operator": "eq", "value": "payment" }
    --   ],
    --   "logic": "all"   -- "all" = AND, "any" = OR
    -- }
    conditions JSONB NOT NULL DEFAULT '{"conditions":[],"logic":"all"}'::jsonb,
    -- Approval chain
    approval_type VARCHAR(50) NOT NULL DEFAULT 'single',
    -- 'single': one approver needed
    -- 'sequential': approvers in order
    -- 'parallel': all must approve (no order)
    -- 'threshold': N of M must approve
    -- Approver list (ordered for sequential)
    approver_user_ids UUID[] NOT NULL DEFAULT '{}',
    -- For threshold type: how many needed
    required_approvals INTEGER NOT NULL DEFAULT 1,
    -- Timeout before escalation (minutes, 0 = no timeout)
    timeout_minutes INTEGER NOT NULL DEFAULT 0,
    -- Escalation target (user to escalate to on timeout)
    escalation_user_id UUID REFERENCES api.users(id),
    -- Auto-approve if no response within timeout (vs escalate)
    auto_approve_on_timeout BOOLEAN NOT NULL DEFAULT false,
    -- Delegation allowed
    allow_delegation BOOLEAN NOT NULL DEFAULT true,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_approval_rules_org ON bpe.approval_rules(organization_id);
```

### 3.10 Approval Requests

```sql
CREATE TABLE bpe.approval_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    rule_id UUID NOT NULL REFERENCES bpe.approval_rules(id),
    -- What is being approved
    workflow_execution_id UUID REFERENCES bpe.workflow_executions(id),
    workflow_step_id UUID REFERENCES bpe.workflow_steps(id),
    -- Context for the approver
    title VARCHAR(500) NOT NULL,
    description TEXT,
    context_data JSONB DEFAULT '{}'::jsonb,   -- data relevant to the decision
    -- State
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    -- pending, approved, rejected, escalated, timed_out, cancelled
    -- Who requested
    requested_by UUID NOT NULL REFERENCES api.users(id),
    -- Current approver in chain (for sequential)
    current_approver_index INTEGER NOT NULL DEFAULT 0,
    -- Deadline
    deadline_at TIMESTAMPTZ,
    -- Resolution
    resolved_at TIMESTAMPTZ,
    resolution_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_approval_reqs_org ON bpe.approval_requests(organization_id);
CREATE INDEX idx_approval_reqs_status ON bpe.approval_requests(organization_id, status);
CREATE INDEX idx_approval_reqs_step ON bpe.approval_requests(workflow_step_id);
```

### 3.11 Approval Decisions

```sql
CREATE TABLE bpe.approval_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    request_id UUID NOT NULL REFERENCES bpe.approval_requests(id) ON DELETE CASCADE,
    -- Who decided
    decided_by UUID NOT NULL REFERENCES api.users(id),
    -- Was this a delegation?
    delegated_from UUID REFERENCES api.users(id),
    decision VARCHAR(50) NOT NULL,  -- 'approved', 'rejected', 'abstained'
    notes TEXT,
    decided_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_approval_decisions_req ON bpe.approval_decisions(request_id);
CREATE INDEX idx_approval_decisions_user ON bpe.approval_decisions(decided_by);
```

### 3.12 Integration Credentials

```sql
CREATE TABLE bpe.integration_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    integration_type VARCHAR(100) NOT NULL,  -- 'email', 'xero', 'docusign', etc.
    name VARCHAR(300) NOT NULL,              -- human-readable label
    -- Encrypted credential blob (AES-256-GCM via ring)
    -- Plaintext is JSON: { "api_key": "...", "client_id": "...", "client_secret": "...", ... }
    encrypted_credentials BYTEA NOT NULL,
    -- Nonce for AES-GCM (12 bytes)
    encryption_nonce BYTEA NOT NULL,
    -- Connection test result
    last_test_at TIMESTAMPTZ,
    last_test_success BOOLEAN,
    last_test_error TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES api.users(id),
    UNIQUE(organization_id, integration_type, name)
);

CREATE INDEX idx_int_creds_org ON bpe.integration_credentials(organization_id);
CREATE INDEX idx_int_creds_type ON bpe.integration_credentials(organization_id, integration_type);
```

### 3.13 Audit Events

```sql
CREATE TABLE bpe.audit_events (
    id BIGSERIAL,                           -- NOT UUID for performance
    organization_id UUID NOT NULL,          -- no FK for partition independence
    -- What happened
    event_type VARCHAR(100) NOT NULL,
    -- 'workflow.created', 'workflow.started', 'workflow.completed',
    -- 'step.started', 'step.completed', 'step.failed',
    -- 'approval.requested', 'approval.decided',
    -- 'entity.created', 'entity.updated', 'entity.archived',
    -- 'integration.called', 'integration.failed',
    -- 'credential.created', 'credential.tested',
    -- 'reversal.initiated', 'reversal.completed'
    -- Subject
    resource_type VARCHAR(100) NOT NULL,    -- 'workflow_execution', 'workflow_step', 'entity', etc.
    resource_id UUID NOT NULL,
    -- Actor
    actor_user_id UUID,
    actor_type VARCHAR(50) NOT NULL DEFAULT 'user',  -- 'user', 'system', 'scheduler', 'integration'
    -- Change data
    before_state JSONB,                     -- state before change (null for creates)
    after_state JSONB,                      -- state after change
    -- Additional context
    metadata JSONB DEFAULT '{}'::jsonb,
    ip_address INET,
    -- Reversal tracking
    is_reversed BOOLEAN NOT NULL DEFAULT false,
    reversed_by_event_id BIGINT,
    reversal_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

CREATE INDEX idx_audit_org ON bpe.audit_events(organization_id);
CREATE INDEX idx_audit_resource ON bpe.audit_events(resource_type, resource_id);
CREATE INDEX idx_audit_type ON bpe.audit_events(event_type);
CREATE INDEX idx_audit_actor ON bpe.audit_events(actor_user_id) WHERE actor_user_id IS NOT NULL;

-- Partitions per month
CREATE TABLE bpe.audit_events_2026_04 PARTITION OF bpe.audit_events
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');
CREATE TABLE bpe.audit_events_2026_05 PARTITION OF bpe.audit_events
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');
-- ... auto-created by migration code for future months
```

### 3.14 Learned Sequences

```sql
CREATE TABLE bpe.learned_sequences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    -- What kind of task this sequence applies to
    task_category VARCHAR(200) NOT NULL,   -- e.g., 'new_hire_onboarding', 'supplier_payment'
    -- Applicable entity types
    entity_type_names VARCHAR(100)[] DEFAULT '{}',
    -- The learned step sequence
    -- Array of: { "order": 1, "name": "...", "type": "manual|automated|...",
    --             "estimated_duration_minutes": 30, "integration_type": null }
    steps JSONB NOT NULL,
    -- Source execution
    source_execution_id UUID REFERENCES bpe.workflow_executions(id),
    -- Effectiveness tracking
    times_suggested INTEGER NOT NULL DEFAULT 0,
    times_accepted INTEGER NOT NULL DEFAULT 0,
    times_modified INTEGER NOT NULL DEFAULT 0,  -- accepted with changes
    times_rejected INTEGER NOT NULL DEFAULT 0,
    avg_completion_minutes DOUBLE PRECISION,
    -- Embedding for semantic similarity search (via goal-rag)
    embedding_text TEXT,    -- text sent to embedder
    -- Version
    version INTEGER NOT NULL DEFAULT 1,
    superseded_by UUID REFERENCES bpe.learned_sequences(id),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_learned_seq_org ON bpe.learned_sequences(organization_id);
CREATE INDEX idx_learned_seq_category ON bpe.learned_sequences(organization_id, task_category);
CREATE INDEX idx_learned_seq_active ON bpe.learned_sequences(organization_id, is_active)
    WHERE is_active = true;
```

### 3.15 Report Templates

```sql
CREATE TABLE bpe.report_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID,  -- NULL = system-wide template
    name VARCHAR(300) NOT NULL,
    description TEXT,
    category VARCHAR(100) NOT NULL,  -- 'hr', 'finance', 'operations', 'compliance'
    -- SQL template with $1=org_id, $2..$N=parameters
    sql_template TEXT NOT NULL,
    -- Parameter definitions: [{ "name": "grade", "type": "string", "label": "Employee Grade", "required": true }]
    parameters JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Output column definitions for formatting
    columns JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_report_templates_org ON bpe.report_templates(organization_id);
```

### 3.16 Notification Queue

```sql
CREATE TABLE bpe.notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    recipient_user_id UUID NOT NULL REFERENCES api.users(id),
    -- What generated this notification
    source_type VARCHAR(100) NOT NULL,  -- 'approval_request', 'step_assigned', 'workflow_completed'
    source_id UUID NOT NULL,
    -- Content
    title VARCHAR(500) NOT NULL,
    body TEXT,
    -- Delivery
    channel VARCHAR(50) NOT NULL DEFAULT 'in_app',  -- 'in_app', 'email', 'both'
    is_read BOOLEAN NOT NULL DEFAULT false,
    read_at TIMESTAMPTZ,
    -- Email delivery tracking
    email_sent BOOLEAN NOT NULL DEFAULT false,
    email_sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_notifications_recipient ON bpe.notifications(recipient_user_id, is_read);
CREATE INDEX idx_notifications_org ON bpe.notifications(organization_id);
```

---

## 4. API Design

All BPE endpoints are served under `/bpe/api/` and require a valid JWT (same token from goal-rag login).

### 4.1 Workflow Endpoints

```
POST   /bpe/api/workflows/decompose
  Body: { "prompt": "Onboard new employee John Smith as Grade 3 engineer",
          "target_entity_id": "uuid-optional",
          "context": { "entity_type": "employee", ... } }
  Response: { "execution_id": "uuid", "steps": [...], "integrations_needed": [...],
              "source_documents": [...] }
  Action: Calls Gemini + RAG to decompose, creates draft execution with steps.

POST   /bpe/api/workflows/executions/:id/confirm
  Body: { "steps": [...modified steps...] }   -- user may reorder/add/remove
  Action: Transitions execution from 'draft' to 'confirmed'. Validates dependencies.

POST   /bpe/api/workflows/executions/:id/start
  Action: Transitions from 'confirmed' to 'running'. Begins executing ready steps.

POST   /bpe/api/workflows/executions/:id/pause
POST   /bpe/api/workflows/executions/:id/resume
POST   /bpe/api/workflows/executions/:id/cancel

GET    /bpe/api/workflows/executions
  Query: ?organization_id=slug&status=running&page=1&per_page=20
  Response: Paginated list of workflow executions.

GET    /bpe/api/workflows/executions/:id
  Response: Full execution detail with all steps, current status.

GET    /bpe/api/workflows/executions/:id/timeline
  Response: Ordered audit events for this execution.

-- Step-level actions
POST   /bpe/api/workflows/steps/:id/complete
  Body: { "output_data": { ... } }
  Action: Mark manual step as completed.

POST   /bpe/api/workflows/steps/:id/skip
  Body: { "reason": "Not applicable" }

POST   /bpe/api/workflows/steps/:id/retry
  Action: Retry a failed step.

POST   /bpe/api/workflows/steps/:id/assign
  Body: { "user_id": "uuid" }

-- Workflow definitions (templates)
GET    /bpe/api/workflows/definitions
  Query: ?organization_id=slug&category=hr

POST   /bpe/api/workflows/definitions
  Body: { "name": "...", "category": "hr", "step_templates": [...] }

PUT    /bpe/api/workflows/definitions/:id
DELETE /bpe/api/workflows/definitions/:id

POST   /bpe/api/workflows/definitions/:id/execute
  Body: { "target_entity_id": "uuid", "context": { ... } }
  Action: Create execution from definition template.
```

### 4.2 Entity Endpoints

```
-- Entity types
GET    /bpe/api/entity-types
  Query: ?organization_id=slug
  Response: All entity types for org (system + custom).

POST   /bpe/api/entity-types
  Body: { "name": "contractor", "display_name": "Contractor",
          "custom_fields": [{ "name": "abn", "type": "string", "label": "ABN" }] }

PUT    /bpe/api/entity-types/:id
DELETE /bpe/api/entity-types/:id   -- only non-system types

-- Entities
GET    /bpe/api/entities
  Query: ?organization_id=slug&entity_type=employee&status=active&page=1&per_page=50
  Response: Paginated entities with attribute data.

GET    /bpe/api/entities/:id
  Response: Full entity with relationships and recent interactions.

POST   /bpe/api/entities
  Body: { "entity_type_id": "uuid", "display_name": "John Smith",
          "attributes": { "email": "john@acme.com", "hire_date": "2026-04-01", ... } }

PUT    /bpe/api/entities/:id
  Body: { "attributes": { ... }, "status": "active" }

DELETE /bpe/api/entities/:id   -- soft delete (sets status='archived')

-- Relationships
GET    /bpe/api/entities/:id/relationships
POST   /bpe/api/entities/:id/relationships
  Body: { "target_entity_id": "uuid", "relationship_type": "reports_to" }
DELETE /bpe/api/entity-relationships/:id

-- Interactions
GET    /bpe/api/entities/:id/interactions
  Query: ?page=1&per_page=20
  Response: Paginated interaction history.

POST   /bpe/api/entities/:id/interactions
  Body: { "interaction_type": "note", "summary": "Called about invoice", "details": {} }
```

### 4.3 Approval Endpoints

```
-- Rules
GET    /bpe/api/approval-rules
  Query: ?organization_id=slug
POST   /bpe/api/approval-rules
PUT    /bpe/api/approval-rules/:id
DELETE /bpe/api/approval-rules/:id

-- Requests (approver-facing)
GET    /bpe/api/approvals/pending
  Query: ?organization_id=slug
  Response: Approval requests where current user is the active approver.

GET    /bpe/api/approvals/:id
  Response: Full approval request with context and decision history.

POST   /bpe/api/approvals/:id/decide
  Body: { "decision": "approved", "notes": "Looks good" }

POST   /bpe/api/approvals/:id/delegate
  Body: { "delegate_to_user_id": "uuid" }
```

### 4.4 Integration Endpoints

```
GET    /bpe/api/integrations/types
  Response: Available integration adapter types with required credential fields.

GET    /bpe/api/integrations/credentials
  Query: ?organization_id=slug
  Response: Configured integrations (credentials redacted).

POST   /bpe/api/integrations/credentials
  Body: { "integration_type": "xero", "name": "ACME Xero",
          "credentials": { "client_id": "...", "client_secret": "..." } }
  Action: Encrypts and stores credentials.

POST   /bpe/api/integrations/credentials/:id/test
  Response: { "success": true/false, "error": "..." }

DELETE /bpe/api/integrations/credentials/:id
```

### 4.5 Audit Endpoints

```
GET    /bpe/api/audit/events
  Query: ?organization_id=slug&resource_type=workflow_execution&resource_id=uuid
          &event_type=step.completed&from=2026-04-01&to=2026-04-30&page=1&per_page=50
  Response: Paginated audit events.

GET    /bpe/api/audit/entity/:entity_id
  Response: All audit events related to an entity across all workflows.

POST   /bpe/api/audit/reversal
  Body: { "event_id": 12345, "reason": "Payment was incorrect" }
  Action: Creates compensating records, marks original as reversed.
```

### 4.6 Reporting Endpoints

```
POST   /bpe/api/reports/query
  Body: { "question": "Show me all employees at grade 3 hired in the last 6 months" }
  Response: { "sql": "...", "columns": [...], "data": [...], "summary": "..." }
  Action: Gemini translates NL to SQL, executes read-only, formats response.

GET    /bpe/api/reports/templates
  Query: ?organization_id=slug&category=hr

POST   /bpe/api/reports/templates/:id/execute
  Body: { "parameters": { "grade": "3" } }
  Response: { "columns": [...], "data": [...] }

POST   /bpe/api/reports/export
  Body: { "report_data": [...], "format": "csv", "title": "..." }
  Response: File download (CSV/Excel).
```

### 4.7 Notification Endpoints

```
GET    /bpe/api/notifications
  Query: ?unread_only=true&page=1&per_page=20
  Response: Paginated notifications for current user.

POST   /bpe/api/notifications/:id/read
POST   /bpe/api/notifications/read-all
GET    /bpe/api/notifications/count
  Response: { "unread": 5 }
```

### 4.8 Health/Info

```
GET    /bpe/api/health
  Response: { "status": "ok", "version": "0.1.0", "db": "connected" }

GET    /bpe/api/info
  Response: { "name": "bpe-server", "version": "...", "endpoints": { ... } }
```

---

## 5. Module Architecture

### 5.1 bpe-core Internal Module Map

```
bpe_core::
├── config::BpeConfig                    # Server config (port, DB, encryption key, RAG URL)
├── error::{BpeError, Result}            # Error enum with Axum IntoResponse
├── auth::                               # JWT validation only (no token issuance)
│   ├── validate_jwt(token) -> Claims
│   ├── require_auth (Axum middleware fn)
│   └── Claims { user_id, email, organization_id, is_platform_admin }
├── db::
│   ├── pool::PgPool                     # Connection pool (same pattern as goal-rag)
│   └── migrations::run_migrations()     # CREATE SCHEMA + all tables IF NOT EXISTS
├── workflow::
│   ├── engine::WorkflowEngine           # Core orchestrator
│   │   ├── decompose(prompt, org_id) -> DraftExecution
│   │   ├── confirm(execution_id, modified_steps)
│   │   ├── start(execution_id)
│   │   ├── advance(execution_id)        # Called after step completion
│   │   ├── pause/resume/cancel(execution_id)
│   │   └── tick()                        # Background: check timeouts, advance ready steps
│   ├── state_machine::StepStateMachine
│   │   ├── can_transition(from, to) -> bool
│   │   ├── transition(step_id, new_status)
│   │   └── VALID_TRANSITIONS: HashMap    # pending->ready, ready->in_progress, etc.
│   ├── decomposer::GoalDecomposer
│   │   ├── decompose(prompt, org_id) -> Vec<StepTemplate>
│   │   ├── query_rag(prompt, org_id) -> Vec<Document>   # calls goal-rag /api/v2/query
│   │   ├── detect_integrations(steps) -> Vec<IntegrationRequirement>
│   │   └── build_decomposition_prompt(prompt, rag_context) -> String
│   ├── scheduler::StepScheduler
│   │   ├── get_ready_steps(execution_id) -> Vec<Step>
│   │   ├── resolve_dependencies(steps) -> DAG
│   │   └── estimate_completion(execution_id) -> DateTime
│   └── models::{WorkflowDefinition, WorkflowExecution, WorkflowStep, StepType, StepStatus, ...}
├── entity::
│   ├── registry::EntityTypeRegistry
│   │   ├── seed_system_types(org_id)     # Creates Employee, Supplier, etc.
│   │   ├── get_type(org_id, name) -> EntityType
│   │   ├── validate_attributes(type_id, attrs) -> Result
│   │   └── SYSTEM_TYPES: [employee, supplier, customer, shareholder, contractor]
│   ├── attributes::AttributeValidator
│   │   ├── validate_field(field_def, value) -> Result
│   │   └── coerce_type(field_def, value) -> Value
│   ├── relationships::RelationshipManager
│   │   ├── add(source, target, type)
│   │   ├── remove(relationship_id)
│   │   ├── get_graph(entity_id, depth) -> Vec<Relationship>
│   │   └── validate_relationship(source_type, target_type, rel_type) -> bool
│   ├── interactions::InteractionTracker
│   │   ├── record(entity_id, interaction)
│   │   └── query(entity_id, filters) -> Vec<Interaction>
│   └── models::{EntityType, Entity, EntityRelationship, EntityInteraction, FieldDef, FieldType}
├── approval::
│   ├── engine::ApprovalEngine
│   │   ├── create_request(step, rule) -> ApprovalRequest
│   │   ├── decide(request_id, user_id, decision)
│   │   ├── delegate(request_id, from, to)
│   │   ├── check_timeouts()              # Background tick
│   │   ├── escalate(request_id)
│   │   └── evaluate_rule(rule, context) -> bool   # Should this rule apply?
│   ├── rules::RuleEvaluator
│   │   ├── evaluate(conditions, context) -> bool
│   │   ├── Operators: eq, neq, gt, gte, lt, lte, in, contains, matches
│   │   └── Logic: all (AND), any (OR)
│   ├── escalation::EscalationManager
│   │   ├── should_escalate(request) -> bool
│   │   ├── escalate(request)
│   │   └── auto_approve(request)         # When configured
│   └── models::{ApprovalRule, ApprovalRequest, ApprovalDecision, ApprovalStatus, ...}
├── integration::
│   ├── registry::AdapterRegistry
│   │   ├── register(type_name, factory)
│   │   ├── get(type_name) -> Box<dyn IntegrationAdapter>
│   │   └── list_types() -> Vec<AdapterInfo>
│   ├── credentials::CredentialManager
│   │   ├── store(org_id, type, creds)    # Encrypts with AES-256-GCM
│   │   ├── retrieve(cred_id) -> Credentials   # Decrypts
│   │   ├── test(cred_id) -> TestResult
│   │   └── delete(cred_id)
│   ├── adapter::IntegrationAdapter (trait)
│   │   ├── adapter_type() -> &str
│   │   ├── required_credentials() -> Vec<CredentialField>
│   │   ├── test_connection(creds) -> Result
│   │   ├── execute(action, params, creds) -> AdapterResult
│   │   └── available_actions() -> Vec<ActionDef>
│   ├── adapters::
│   │   ├── email::EmailAdapter           # SMTP send, template rendering
│   │   ├── webhook::WebhookAdapter       # Generic HTTP call
│   │   ├── xero::XeroAdapter             # Invoice creation, payment recording
│   │   ├── docusign::DocuSignAdapter     # Send for signature, check status
│   │   └── quickbooks::QuickBooksAdapter # Similar to Xero
│   └── models::{IntegrationCredential, AdapterInfo, AdapterResult, CredentialField}
├── audit::
│   ├── logger::AuditLogger
│   │   ├── log(event)                    # Inserts into bpe.audit_events
│   │   ├── log_change(resource, before, after)
│   │   └── batch_log(events)             # Bulk insert
│   ├── reversal::ReversalEngine
│   │   ├── reverse(event_id, reason) -> ReversalResult
│   │   ├── can_reverse(event_id) -> bool
│   │   └── create_compensation(original_event) -> AuditEvent
│   ├── query::AuditQuery
│   │   ├── by_resource(type, id) -> Vec<AuditEvent>
│   │   ├── by_entity(entity_id) -> Vec<AuditEvent>
│   │   ├── by_workflow(execution_id) -> Vec<AuditEvent>
│   │   └── search(filters, pagination) -> Page<AuditEvent>
│   └── models::{AuditEvent, ReversalResult}
├── knowledge::
│   ├── learner::SequenceLearner
│   │   ├── learn_from_execution(execution_id)     # Called on workflow completion
│   │   ├── categorize(prompt) -> String            # LLM categorizes the task
│   │   └── extract_sequence(execution) -> LearnedSequence
│   ├── suggester::SequenceSuggester
│   │   ├── suggest(prompt, org_id) -> Vec<SuggestedSequence>
│   │   ├── find_similar(prompt, org_id) -> Vec<LearnedSequence>  # Semantic search
│   │   ├── record_feedback(seq_id, accepted|modified|rejected)
│   │   └── best_for_category(category, org_id) -> Option<LearnedSequence>
│   └── models::{LearnedSequence, SuggestedSequence, SequenceFeedback}
├── reporting::
│   ├── engine::ReportEngine
│   │   ├── natural_language_query(question, org_id) -> ReportResult
│   │   ├── execute_template(template_id, params) -> ReportResult
│   │   └── build_nl_to_sql_prompt(question, schema_context) -> String
│   ├── templates::TemplateManager
│   │   ├── seed_defaults(org_id)          # Pre-built reports
│   │   ├── get(template_id) -> ReportTemplate
│   │   └── list(org_id, category) -> Vec<ReportTemplate>
│   ├── export::Exporter
│   │   ├── to_csv(data) -> Vec<u8>
│   │   ├── to_excel(data) -> Vec<u8>     # Simple CSV-based, not full XLSX
│   │   └── to_pdf(data) -> Vec<u8>       # Minimal table layout
│   └── models::{ReportTemplate, ReportResult, ReportColumn}
└── llm::
    ├── gemini::GeminiClient
    │   ├── new(auth, location, model)
    │   ├── generate(prompt) -> String
    │   └── generate_structured<T>(prompt) -> T   # JSON mode for typed responses
    └── prompts::
        ├── DECOMPOSITION_SYSTEM_PROMPT
        ├── DECOMPOSITION_USER_TEMPLATE
        ├── NL_TO_SQL_SYSTEM_PROMPT
        ├── CATEGORIZATION_PROMPT
        └── STEP_DETAIL_PROMPT
```

### 5.2 bpe-server Route Composition

```rust
// bpe-server/src/routes/mod.rs
pub fn bpe_routes() -> Router<AppState> {
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/info", get(health::info));

    let protected = Router::new()
        // Workflows
        .route("/workflows/decompose", post(workflows::decompose))
        .route("/workflows/executions", get(workflows::list_executions))
        .route("/workflows/executions/:id", get(workflows::get_execution))
        .route("/workflows/executions/:id/confirm", post(workflows::confirm))
        .route("/workflows/executions/:id/start", post(workflows::start))
        .route("/workflows/executions/:id/pause", post(workflows::pause))
        .route("/workflows/executions/:id/resume", post(workflows::resume))
        .route("/workflows/executions/:id/cancel", post(workflows::cancel))
        .route("/workflows/executions/:id/timeline", get(workflows::timeline))
        .route("/workflows/steps/:id/complete", post(workflows::complete_step))
        .route("/workflows/steps/:id/skip", post(workflows::skip_step))
        .route("/workflows/steps/:id/retry", post(workflows::retry_step))
        .route("/workflows/steps/:id/assign", post(workflows::assign_step))
        .route("/workflows/definitions", get(workflows::list_definitions))
        .route("/workflows/definitions", post(workflows::create_definition))
        .route("/workflows/definitions/:id", put(workflows::update_definition))
        .route("/workflows/definitions/:id", delete(workflows::delete_definition))
        .route("/workflows/definitions/:id/execute", post(workflows::execute_definition))
        // Entities
        .route("/entity-types", get(entity_types::list))
        .route("/entity-types", post(entity_types::create))
        .route("/entity-types/:id", put(entity_types::update))
        .route("/entity-types/:id", delete(entity_types::delete))
        .route("/entities", get(entities::list))
        .route("/entities", post(entities::create))
        .route("/entities/:id", get(entities::get))
        .route("/entities/:id", put(entities::update))
        .route("/entities/:id", delete(entities::delete))
        .route("/entities/:id/relationships", get(entities::list_relationships))
        .route("/entities/:id/relationships", post(entities::add_relationship))
        .route("/entity-relationships/:id", delete(entities::remove_relationship))
        .route("/entities/:id/interactions", get(entities::list_interactions))
        .route("/entities/:id/interactions", post(entities::add_interaction))
        // Approvals
        .route("/approval-rules", get(approvals::list_rules))
        .route("/approval-rules", post(approvals::create_rule))
        .route("/approval-rules/:id", put(approvals::update_rule))
        .route("/approval-rules/:id", delete(approvals::delete_rule))
        .route("/approvals/pending", get(approvals::pending))
        .route("/approvals/:id", get(approvals::get_request))
        .route("/approvals/:id/decide", post(approvals::decide))
        .route("/approvals/:id/delegate", post(approvals::delegate))
        // Integrations
        .route("/integrations/types", get(integrations::list_types))
        .route("/integrations/credentials", get(integrations::list_credentials))
        .route("/integrations/credentials", post(integrations::create_credential))
        .route("/integrations/credentials/:id/test", post(integrations::test_credential))
        .route("/integrations/credentials/:id", delete(integrations::delete_credential))
        // Audit
        .route("/audit/events", get(audit::list_events))
        .route("/audit/entity/:entity_id", get(audit::entity_events))
        .route("/audit/reversal", post(audit::reverse_event))
        // Reports
        .route("/reports/query", post(reports::nl_query))
        .route("/reports/templates", get(reports::list_templates))
        .route("/reports/templates/:id/execute", post(reports::execute_template))
        .route("/reports/export", post(reports::export))
        // Notifications
        .route("/notifications", get(notifications::list))
        .route("/notifications/count", get(notifications::count))
        .route("/notifications/:id/read", post(notifications::mark_read))
        .route("/notifications/read-all", post(notifications::mark_all_read))
        .layer(middleware::from_fn(bpe_core::auth::require_auth));

    Router::new()
        .nest("/bpe/api", public.merge(protected))
}
```

---

## 6. Workflow Engine Design

### 6.1 Execution Lifecycle

```
                   User submits goal/task description
                                │
                                ▼
                    ┌───────────────────────┐
                    │   decompose()         │
                    │   1. Query RAG for    │
                    │      relevant SOPs    │
                    │   2. Query learned    │
                    │      sequences        │
                    │   3. Call Gemini to   │
                    │      generate steps   │
                    │   4. Detect needed    │
                    │      integrations     │
                    │   5. Create draft     │
                    │      execution        │
                    └──────────┬────────────┘
                               │
                               ▼
                        ┌──────────┐
                  ┌─────│  DRAFT   │
                  │     └──────────┘
                  │     User reviews steps, modifies order,
                  │     adds/removes steps, provides credentials
                  │            │
                  │            ▼  confirm()
                  │     ┌──────────────┐
                  │     │  CONFIRMED   │
                  │     └──────────────┘
                  │            │
                  │            ▼  start()
                  │     ┌──────────────┐     pause()     ┌──────────────┐
                  │     │   RUNNING    │────────────────►│   PAUSED     │
                  │     └──────────────┘                 └──────────────┘
                  │            │                                │
                  │            │ All steps done                 │ resume()
                  │            ▼                                │
                  │     ┌──────────────┐                       │
                  │     │  COMPLETED   │◄──────────────────────┘
                  │     └──────────────┘
                  │
                  │  cancel() from any state
                  │     ┌──────────────┐
                  └────►│  CANCELLED   │
                        └──────────────┘

                  On unrecoverable error:
                        ┌──────────────┐
                        │   FAILED     │
                        └──────────────┘
```

### 6.2 Step State Machine

```
                    ┌─────────┐
                    │ PENDING │  (created, dependencies not yet met)
                    └────┬────┘
                         │  all dependencies completed
                         ▼
                    ┌─────────┐
                    │  READY  │  (can be started)
                    └────┬────┘
                         │  engine.advance() or manual start
                         ▼
              ┌──────────────────────┐
              │     IN_PROGRESS      │
              └──────┬───────┬───────┘
                     │       │
          ┌──────────┘       └──────────┐
          │                             │
          ▼                             ▼
  ┌────────────────┐          ┌─────────────────────┐
  │WAITING_APPROVAL│          │WAITING_INTEGRATION   │
  └───────┬────────┘          └──────────┬───────────┘
          │  approval.decided             │  adapter.response
          ▼                              ▼
  ┌──────────────┐               ┌──────────────┐
  │  COMPLETED   │               │  COMPLETED   │
  └──────────────┘               └──────────────┘

  From any active state:
  ┌──────────┐     ┌──────────┐
  │  FAILED  │     │  SKIPPED │
  └──────────┘     └──────────┘
       │
       ▼ retry (if retry_count < max_retries)
  ┌─────────┐
  │  READY  │
  └─────────┘
```

### 6.3 Step Type Execution

```rust
// Pseudocode for WorkflowEngine::execute_step()
async fn execute_step(&self, step: &WorkflowStep) -> Result<()> {
    match step.step_type {
        StepType::Manual => {
            // Transition to in_progress
            // Create notification for assigned_to user
            // Wait for user to call /steps/:id/complete
        }
        StepType::Approval => {
            // Look up approval rule
            // Create ApprovalRequest
            // Transition to waiting_approval
            // Create notifications for approvers
            // ApprovalEngine handles the rest
        }
        StepType::Integration => {
            // Look up adapter from registry
            // Retrieve decrypted credentials
            // Call adapter.execute(action, params, creds)
            // Store integration_result
            // Transition to completed or failed
        }
        StepType::Automated => {
            // Same as Integration but simpler (internal system action)
            // e.g., create entity, update record
        }
        StepType::LlmAction => {
            // Call Gemini to generate content (e.g., draft email)
            // Store in output_data
            // Transition to completed (or to manual if user review needed)
        }
        StepType::SubWorkflow => {
            // Create child WorkflowExecution
            // Transition to in_progress
            // Monitor child completion
        }
    }
}
```

### 6.4 Background Ticker

The workflow engine runs a background task (`tokio::spawn`) that ticks every 30 seconds:

```rust
async fn tick(&self) {
    // 1. Find steps with status='ready' → execute them
    // 2. Find steps with status='pending' → check if dependencies met → transition to 'ready'
    // 3. Check approval timeouts → escalate or auto-approve
    // 4. Check integration retries → retry failed steps where retry_count < max_retries
    // 5. Check running executions with all steps completed → mark execution completed
    // 6. Learn from completed executions → store learned sequences
}
```

### 6.5 Dependency Resolution

Steps declare dependencies as an array of `step_order` values. The scheduler builds a DAG:

```rust
fn get_ready_steps(execution_id: Uuid) -> Vec<WorkflowStep> {
    // SELECT * FROM bpe.workflow_steps
    // WHERE execution_id = $1
    //   AND status = 'pending'
    //   AND NOT EXISTS (
    //       SELECT 1 FROM bpe.workflow_steps dep
    //       WHERE dep.execution_id = $1
    //         AND dep.step_order = ANY(workflow_steps.dependencies)
    //         AND dep.status NOT IN ('completed', 'skipped')
    //   )
    // Transition matching steps from 'pending' to 'ready'
}
```

---

## 7. Entity System Design

### 7.1 Pre-defined Entity Types

On first access to BPE for an organization (or explicit seed), the system creates these system entity types:

| Type Name | Display Name | Core Fields |
|---|---|---|
| `employee` | Employee | `first_name`, `last_name`, `email`, `position`, `department`, `grade`, `hire_date`, `manager_entity_id` |
| `supplier` | Supplier/Vendor | `company_name`, `contact_name`, `email`, `phone`, `abn`, `payment_terms`, `category` |
| `customer` | Customer | `company_name`, `contact_name`, `email`, `phone`, `account_number`, `billing_address` |
| `shareholder` | Shareholder | `name`, `email`, `share_count`, `share_class`, `acquisition_date` |
| `contractor` | Contractor | `name`, `company_name`, `email`, `abn`, `contract_start`, `contract_end`, `hourly_rate` |

### 7.2 Field Type System

```rust
pub enum FieldType {
    String,         // VARCHAR text
    Text,           // Long text
    Integer,        // i64
    Decimal,        // f64
    Boolean,
    Date,           // YYYY-MM-DD
    DateTime,       // ISO 8601
    Email,          // Validated email format
    Phone,          // E.164 format
    Uuid,           // Reference to another entity or user
    Currency,       // { "amount": 100.00, "currency": "AUD" }
    Enum(Vec<String>),  // One of predefined values
    JsonObject,     // Free-form JSON
}
```

Field definitions are stored as JSONB:
```json
{
    "name": "hire_date",
    "type": "date",
    "label": "Hire Date",
    "required": true,
    "description": "Date the employee started",
    "default_value": null,
    "validation": null
}
```

### 7.3 Entity-User Linking

Employee entities can be linked to `api.users` via `linked_user_id`. This enables:
- Automatic entity creation when a user is created (via CDC/trigger)
- Approval rules that reference entities (e.g., "the employee's manager must approve")
- Reporting that joins entity attributes with user activity data

### 7.4 Relationship Types

Predefined relationship types (organizations can add custom ones):

| Relationship | Source Type | Target Type | Notes |
|---|---|---|---|
| `reports_to` | employee | employee | Org chart hierarchy |
| `manages` | employee | employee | Inverse of reports_to |
| `contracted_by` | contractor | (org) | Contract relationship |
| `supplies_to` | supplier | (org) | Vendor relationship |
| `invoices` | supplier | (org) | Financial relationship |
| `customer_of` | customer | (org) | Customer relationship |
| `holds_shares_in` | shareholder | (org) | Ownership |
| `custom` | any | any | Org-defined |

---

## 8. Approval Engine Design

### 8.1 Rule Evaluation Flow

```
Step transitions to approval type
            │
            ▼
┌─────────────────────────┐
│ Find matching rules     │  SELECT FROM bpe.approval_rules
│ for this org + context  │  WHERE conditions match step context
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│ Evaluate conditions     │  RuleEvaluator::evaluate(rule.conditions, step.context)
│ against step context    │  Context: { amount, entity_type, department, step_type, ... }
└────────────┬────────────┘
             │ Rule matches
             ▼
┌─────────────────────────┐
│ Create ApprovalRequest  │  Insert into bpe.approval_requests
│ Set deadline            │  deadline_at = now() + rule.timeout_minutes
│ Notify first approver   │  Insert notification
└────────────┬────────────┘
             │
             ▼
       ┌──────────┐
       │ PENDING  │◄───────────────────────────────────┐
       └────┬─────┘                                     │
            │                                           │
            ▼ Approver decides                          │
┌────────────────────────┐                              │
│ Record decision in     │                              │
│ approval_decisions     │                              │
└────────────┬───────────┘                              │
             │                                          │
             ▼                                          │
     ┌───────────────┐                                  │
     │ approval_type │                                  │
     └───┬───┬───┬───┘                                  │
         │   │   │                                      │
    single│  │seq│  threshold                           │
         │   │   │                                      │
         ▼   │   ▼                                      │
  Done   │   │  Count decisions >= required_approvals?  │
         │   │       │                                  │
         │   ▼       │ no                               │
         │  Next     └──────────────────────────────────┘
         │  approver?
         │   │ yes → notify next, increment index
         │   │ no  → all approved
         ▼   ▼
  ┌──────────────┐          ┌──────────────┐
  │   APPROVED   │          │   REJECTED   │
  └──────────────┘          └──────────────┘
         │                        │
         ▼                        ▼
  Step transitions          Step transitions
  to COMPLETED              to FAILED
```

### 8.2 Condition DSL

```json
{
    "conditions": [
        { "field": "step.integration_config.amount", "operator": "gt", "value": 10000 },
        { "field": "step.integration_type", "operator": "eq", "value": "payment" },
        { "field": "target_entity.attributes.department", "operator": "eq", "value": "engineering" }
    ],
    "logic": "all"
}
```

Supported operators: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`, `not_in`, `contains`, `starts_with`, `is_null`, `is_not_null`.

Context available to conditions:
- `step.*` — all fields from the workflow step
- `execution.*` — fields from the workflow execution
- `target_entity.*` — entity attributes if execution has target_entity_id
- `initiated_by.*` — user info of who started the workflow

### 8.3 Delegation

When an approver delegates to another user:
1. Insert delegation record in `approval_decisions` with `delegated_from`
2. Update `approval_requests.current_approver_index` (for sequential)
3. Notify the delegate
4. The delegate's decision counts as the original approver's

---

## 9. Integration Framework

### 9.1 Adapter Trait

```rust
#[async_trait]
pub trait IntegrationAdapter: Send + Sync {
    /// Unique type identifier (e.g., "email", "xero")
    fn adapter_type(&self) -> &str;

    /// Human-readable name
    fn display_name(&self) -> &str;

    /// What credentials are needed
    fn required_credentials(&self) -> Vec<CredentialField>;

    /// Available actions this adapter supports
    fn available_actions(&self) -> Vec<ActionDef>;

    /// Test if credentials are valid
    async fn test_connection(&self, credentials: &serde_json::Value) -> Result<TestResult>;

    /// Execute an action
    async fn execute(
        &self,
        action: &str,
        params: &serde_json::Value,
        credentials: &serde_json::Value,
    ) -> Result<AdapterResult>;
}
```

### 9.2 Adapter Registry

```rust
pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn IntegrationAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        let mut registry = Self { adapters: HashMap::new() };
        registry.register(Arc::new(EmailAdapter::new()));
        registry.register(Arc::new(WebhookAdapter::new()));
        registry.register(Arc::new(XeroAdapter::new()));
        registry.register(Arc::new(DocuSignAdapter::new()));
        registry.register(Arc::new(QuickBooksAdapter::new()));
        registry
    }
}
```

### 9.3 Email Adapter Actions

| Action | Params | Notes |
|---|---|---|
| `send_email` | `{ to, cc, bcc, subject, body_html, body_text, attachments }` | SMTP send |
| `send_template` | `{ to, template_name, variables }` | Render template + send |

### 9.4 Xero Adapter Actions

| Action | Params | Notes |
|---|---|---|
| `create_invoice` | `{ contact_id, line_items, due_date, reference }` | Create sales invoice |
| `create_bill` | `{ contact_id, line_items, due_date }` | Create purchase bill |
| `record_payment` | `{ invoice_id, amount, date, account_code }` | Record payment against invoice |
| `get_contacts` | `{ name_filter }` | Search contacts |
| `get_accounts` | `{}` | List chart of accounts |

### 9.5 Webhook Adapter Actions

| Action | Params | Notes |
|---|---|---|
| `http_request` | `{ url, method, headers, body, timeout_ms }` | Generic HTTP call |
| `webhook_notify` | `{ url, payload }` | POST JSON to webhook URL |

### 9.6 Credential Encryption

```rust
// bpe-core/src/integration/credentials.rs
use ring::aead::{Aead, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};

pub struct CredentialManager {
    encryption_key: LessSafeKey,  // Derived from BPE_ENCRYPTION_KEY env var
    pool: PgPool,
}

impl CredentialManager {
    pub fn encrypt(&self, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plaintext.to_vec();
        self.encryption_key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out).unwrap();
        (in_out, nonce_bytes.to_vec())
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce_bytes: &[u8]) -> Vec<u8> {
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes).unwrap();
        let mut in_out = ciphertext.to_vec();
        self.encryption_key.open_in_place(nonce, Aad::empty(), &mut in_out).unwrap();
        in_out
    }
}
```

Encryption key: `BPE_ENCRYPTION_KEY` environment variable (32-byte hex-encoded key). Must be set in production. In dev, a default key is used with a warning log.

---

## 10. Audit System Design

### 10.1 Event Recording Strategy

Every mutable operation in BPE calls `AuditLogger::log()` with before/after state. This is not event sourcing in the strict sense (current state is in the primary tables, not derived from events), but events provide a complete history.

```rust
// Usage pattern in every write operation:
async fn update_entity(pool: &PgPool, audit: &AuditLogger, entity_id: Uuid, update: EntityUpdate) {
    let before = get_entity(pool, entity_id).await?;
    let after = apply_update(pool, entity_id, &update).await?;

    audit.log(AuditEvent {
        event_type: "entity.updated".to_string(),
        resource_type: "entity".to_string(),
        resource_id: entity_id,
        actor_user_id: Some(current_user_id),
        actor_type: "user".to_string(),
        before_state: Some(serde_json::to_value(&before)?),
        after_state: Some(serde_json::to_value(&after)?),
        metadata: serde_json::json!({ "changed_fields": update.changed_fields() }),
        ..Default::default()
    }).await?;
}
```

### 10.2 Reversal Mechanism

Reversals are soft: they do not undo database changes. Instead:

1. The original audit event is marked `is_reversed = true`
2. A new audit event of type `reversal.*` is created with `reversed_by_event_id` pointing to the original
3. A compensating record is created where applicable (e.g., a credit note for a reversed payment)
4. The entity/workflow status is updated to reflect the reversal

```rust
async fn reverse(event_id: i64, reason: &str) -> Result<ReversalResult> {
    let original = get_event(event_id)?;

    // Check reversibility
    if original.is_reversed {
        return Err("Already reversed");
    }

    // Create compensation based on event type
    let compensation = match original.event_type.as_str() {
        "integration.payment" => {
            // Create credit note or refund record
            create_compensation_payment(&original)?
        }
        "entity.updated" => {
            // Revert entity to before_state
            restore_entity_state(&original)?
        }
        _ => {
            // Generic: just mark as reversed, no automatic compensation
            None
        }
    };

    // Mark original as reversed
    mark_reversed(event_id)?;

    // Log the reversal event
    audit.log(AuditEvent {
        event_type: format!("reversal.{}", original.event_type),
        resource_type: original.resource_type,
        resource_id: original.resource_id,
        before_state: original.after_state,   // reversal "before" is original "after"
        after_state: original.before_state,    // reversal "after" is original "before"
        metadata: json!({ "reason": reason, "original_event_id": event_id }),
        ..
    })?;
}
```

### 10.3 Partition Management

Audit event partitions are created automatically by the migration code. On startup, the migration checks if partitions exist for the current month and next 3 months, creating any that are missing:

```rust
async fn ensure_partitions(pool: &PgPool) {
    let now = Utc::now();
    for offset in 0..4 {
        let month = now + Duration::months(offset);
        let start = month.format("%Y-%m-01");
        let next = (month + Duration::months(1)).format("%Y-%m-01");
        let name = month.format("bpe.audit_events_%Y_%m");
        // CREATE TABLE IF NOT EXISTS {name} PARTITION OF bpe.audit_events
        //     FOR VALUES FROM ('{start}') TO ('{next}');
    }
}
```

---

## 11. Knowledge Learning Loop

### 11.1 Learning Flow

```
Workflow completes successfully
            │
            ▼
┌───────────────────────────┐
│ SequenceLearner::          │
│ learn_from_execution()     │
│                            │
│ 1. Load execution + steps  │
│ 2. Filter out skipped steps│
│ 3. Normalize step data     │
│    (remove instance-       │
│    specific details)       │
│ 4. Categorize via Gemini   │
│    "What type of task is   │
│     this?"                 │
│    → "new_hire_onboarding" │
│ 5. Check for existing      │
│    sequence in same        │
│    category                │
│ 6. If exists: update stats │
│    (avg_completion, etc.)  │
│    If better: create new   │
│    version, supersede old  │
│    If not: just update     │
│    stats                   │
│ 7. If new: insert          │
│ 8. Generate embedding text │
│    and store (optional,    │
│    via goal-rag API)       │
└───────────────────────────┘
```

### 11.2 Suggestion Flow

```
User submits new goal/task prompt
            │
            ▼
┌───────────────────────────┐
│ SequenceSuggester::        │
│ suggest()                  │
│                            │
│ 1. Categorize prompt       │
│    via Gemini              │
│ 2. Find learned sequences  │
│    matching category       │
│    (exact match first)     │
│ 3. If no exact match:      │
│    semantic search via     │
│    embedding similarity    │
│ 4. Rank by:                │
│    - success_rate          │
│    - times_accepted vs     │
│      times_rejected        │
│    - avg_completion_minutes│
│    - recency               │
│ 5. Return top 3 matches    │
│    as suggestions          │
└────────────┬──────────────┘
             │
             ▼
┌───────────────────────────┐
│ GoalDecomposer::           │
│ decompose()                │
│                            │
│ If learned sequence found: │
│   Use it as starting       │
│   template, let Gemini     │
│   adapt to current context │
│                            │
│ If no sequence found:      │
│   Full LLM decomposition   │
│   with RAG context         │
└───────────────────────────┘
```

### 11.3 Feedback Loop

When a user accepts, modifies, or rejects a suggested sequence:
- `times_accepted++` or `times_modified++` or `times_rejected++`
- If modified: the modified version becomes a new learned sequence (if execution completes)
- If rejected: factor into ranking (lower priority for future suggestions)
- Success rate calculated as: `(accepted + modified) / (accepted + modified + rejected)`

### 11.4 RAG Integration

Learned sequences are indexed into the RAG knowledge base:
1. Generate descriptive text: `"{category}: {step1_name} -> {step2_name} -> ... (avg {X} minutes, {Y}% success rate)"`
2. Call goal-rag's document ingestion API (or directly insert into `rag_chunks` with org isolation)
3. Future RAG queries for similar tasks will surface these learned sequences alongside SOP documents

---

## 12. Frontend Components

### 12.1 New Pages in marshal-ui-react

All BPE pages live under `/crates/marshal-ui-react/src/pages/processes/`.

```
pages/processes/
├── ProcessDashboard.tsx          # Overview: active workflows, pending approvals, recent activity
├── WorkflowBuilder.tsx           # Goal decomposition UI
│   ├── PromptInput               # Text area for goal/task description
│   ├── StepList                  # Draggable list of generated steps
│   ├── StepEditor                # Edit individual step details
│   ├── IntegrationPrompt         # Request credentials for needed integrations
│   └── ConfirmationPanel         # Review and confirm workflow
├── WorkflowExecution.tsx         # Active workflow detail view
│   ├── StepTimeline              # Visual timeline of steps with status
│   ├── StepDetail                # Current step detail + action buttons
│   ├── ApprovalWidget            # Approve/reject inline
│   └── AuditTrail                # Side panel with event log
├── WorkflowList.tsx              # List all workflow executions (filterable)
├── EntityManager.tsx             # Entity CRUD
│   ├── EntityTypeConfig          # Configure entity types + custom fields
│   ├── EntityList                # Searchable entity table
│   ├── EntityDetail              # Single entity view with relationships + interactions
│   └── RelationshipGraph         # Visual graph of entity relationships
├── ApprovalQueue.tsx             # Pending approvals for current user
├── ApprovalRuleManager.tsx       # CRUD for approval rules
├── IntegrationManager.tsx        # Configure external integrations + test connections
├── ReportBuilder.tsx             # Natural language query + template execution
│   ├── NLQueryInput              # "Ask a question about your data"
│   ├── ResultTable               # Formatted results
│   └── ExportButtons             # CSV/Excel/PDF download
├── AuditLog.tsx                  # Searchable audit trail
└── NotificationCenter.tsx        # Notification list + mark read
```

### 12.2 Sidebar Updates

Add a "Processes" section to the existing sidebar in `components/layout/`:

```
Processes
  ├── Dashboard          → /processes
  ├── New Workflow       → /processes/new
  ├── Active Workflows   → /processes/workflows
  ├── My Approvals       → /processes/approvals
  ├── Entities           → /processes/entities
  ├── Integrations       → /processes/integrations
  ├── Reports            → /processes/reports
  └── Audit Log          → /processes/audit
```

### 12.3 Key UI Interactions

**Workflow Builder (decompose flow):**
1. User types goal in text area, clicks "Generate Steps"
2. Loading spinner while Gemini + RAG processes
3. Steps appear as a vertical sortable list (drag handles)
4. Each step shows: name, type badge, estimated duration, dependencies
5. Click step to expand editor (edit name, description, type, assignee, integration)
6. If integrations needed: alert banner "This workflow needs Xero and Email. Configure now?"
7. Integration config modal: enter credentials, test connection
8. Once satisfied, click "Confirm and Start" or "Save as Draft"

**Approval Widget:**
- Card showing: what needs approval, context data, requester, deadline
- Two buttons: "Approve" (green), "Reject" (red)
- Optional notes text area
- "Delegate" link to reassign

---

## 13. Data Flow Diagrams

### 13.1 Goal Decomposition Flow

```
User                    BPE Server              goal-rag             Gemini LLM
 │                          │                       │                     │
 │  POST /decompose         │                       │                     │
 │  { prompt: "Onboard      │                       │                     │
 │    new employee..." }    │                       │                     │
 │─────────────────────────►│                       │                     │
 │                          │                       │                     │
 │                          │  POST /api/v2/query   │                     │
 │                          │  { question: prompt,  │                     │
 │                          │    org_id: slug }     │                     │
 │                          │──────────────────────►│                     │
 │                          │                       │                     │
 │                          │  { chunks: [SOP docs, │                     │
 │                          │    policies, laws] }  │                     │
 │                          │◄──────────────────────│                     │
 │                          │                       │                     │
 │                          │  Find learned         │                     │
 │                          │  sequences (DB)       │                     │
 │                          │                       │                     │
 │                          │  Build prompt with    │                     │
 │                          │  RAG context +        │                     │
 │                          │  learned sequences    │                     │
 │                          │                       │                     │
 │                          │  generateContent()    │                     │
 │                          │─────────────────────────────────────────────►│
 │                          │                       │                     │
 │                          │  { steps: [...],      │                     │
 │                          │    integrations: [...]}│                    │
 │                          │◄─────────────────────────────────────────────│
 │                          │                       │                     │
 │                          │  Create draft         │                     │
 │                          │  execution + steps    │                     │
 │                          │  in bpe schema        │                     │
 │                          │                       │                     │
 │  { execution_id,         │                       │                     │
 │    steps: [...],         │                       │                     │
 │    integrations_needed,  │                       │                     │
 │    source_documents }    │                       │                     │
 │◄─────────────────────────│                       │                     │
```

### 13.2 Workflow Execution Flow

```
User             BPE Server          Approval Engine       Integration Adapter
 │                   │                      │                       │
 │ POST /start       │                      │                       │
 │──────────────────►│                      │                       │
 │                   │                      │                       │
 │                   │ Resolve dependencies │                       │
 │                   │ Mark ready steps     │                       │
 │                   │                      │                       │
 │                   │ Step 1: manual       │                       │
 │  notification     │                      │                       │
 │◄──────────────────│                      │                       │
 │                   │                      │                       │
 │ POST /complete    │                      │                       │
 │──────────────────►│                      │                       │
 │                   │                      │                       │
 │                   │ Step 2: approval     │                       │
 │                   │─────────────────────►│                       │
 │                   │ create request       │                       │
 │                   │                      │                       │
 │                   │ notify approver      │                       │
 │                   │◄─────────────────────│                       │
 │                   │                      │                       │
 │                   │ (approver decides)   │                       │
 │                   │◄─────────────────────│                       │
 │                   │                      │                       │
 │                   │ Step 3: integration  │                       │
 │                   │ (Xero payment)       │                       │
 │                   │──────────────────────────────────────────────►│
 │                   │                      │                       │
 │                   │                      │       result          │
 │                   │◄─────────────────────────────────────────────│
 │                   │                      │                       │
 │                   │ All steps complete   │                       │
 │                   │ Mark execution       │                       │
 │                   │ completed            │                       │
 │                   │                      │                       │
 │                   │ Learn from           │                       │
 │                   │ execution            │                       │
 │  workflow done    │                      │                       │
 │◄──────────────────│                      │                       │
```

### 13.3 Approval Flow

```
BPE Engine         Approval Engine        DB                  Notification
    │                    │                  │                       │
    │ request_approval   │                  │                       │
    │   (step, rule)     │                  │                       │
    │───────────────────►│                  │                       │
    │                    │                  │                       │
    │                    │ INSERT request   │                       │
    │                    │─────────────────►│                       │
    │                    │                  │                       │
    │                    │ INSERT notif     │                       │
    │                    │─────────────────────────────────────────►│
    │                    │                  │                       │
    │                    │                  │   (approver clicks    │
    │                    │                  │    approve)           │
    │                    │◄────────────────────────────────────────│
    │                    │                  │                       │
    │                    │ INSERT decision  │                       │
    │                    │─────────────────►│                       │
    │                    │                  │                       │
    │                    │ Check: enough    │                       │
    │                    │ approvals?       │                       │
    │                    │─────────────────►│                       │
    │                    │                  │                       │
    │  approval_resolved │                  │                       │
    │  (approved)        │                  │                       │
    │◄───────────────────│                  │                       │
    │                    │                  │                       │
    │ advance step       │                  │                       │
    │ to completed       │                  │                       │
```

---

## 14. Security Considerations

### 14.1 Authentication

- BPE validates JWTs using the same HMAC-SHA256 secret as goal-rag (`POSTGREST_JWT_SECRET` env var or default)
- BPE does NOT issue tokens. Users log in via goal-rag's `/api/auth/login`
- The `require_auth` middleware extracts `Claims { user_id, email, organization_id, is_platform_admin }` and injects into request extensions
- All endpoints read `organization_id` from JWT claims (not from request body/query) for primary authorization

### 14.2 Organization Isolation

- Every SQL query includes `WHERE organization_id = $1`
- `organization_id` is extracted from JWT claims, never from user input for authorization purposes
- `is_platform_admin` bypasses org filter (can see all orgs) for admin users only
- Cross-org queries are impossible without admin JWT

### 14.3 Credential Security

- Integration credentials are encrypted with AES-256-GCM before storage
- Encryption key is loaded from `BPE_ENCRYPTION_KEY` env var (32 bytes, hex-encoded)
- Nonce is randomly generated per encryption operation and stored alongside ciphertext
- Credentials are NEVER logged (even at trace level)
- Credentials are NEVER returned in API responses (only metadata: type, name, last_test_at)
- Decryption happens in-memory only when executing an integration step

### 14.4 RBAC

Phase 1 uses the existing `is_platform_admin` flag for admin operations. Phase 2 introduces BPE-specific roles:

| Role | Permissions |
|---|---|
| `bpe_admin` | Full access: manage entity types, approval rules, integrations, view all audit |
| `bpe_manager` | Create/manage workflows, approve, view entities, run reports |
| `bpe_user` | Execute assigned steps, view own workflows, view entities |
| `bpe_viewer` | Read-only access to workflows, entities, reports |

Future: Stored in `bpe.user_roles` table, checked via middleware.

### 14.5 Audit Integrity

- Audit events are INSERT-only (no UPDATE/DELETE on `bpe.audit_events`)
- The application user should have INSERT but not UPDATE/DELETE on the audit table
- Reversals create new events (never modify originals)
- IP address recorded on every audit event (from request headers)
- Timestamps are server-generated (`DEFAULT now()`), never client-supplied

### 14.6 SQL Injection Prevention

- All queries use parameterized queries ($1, $2, ...) via tokio-postgres
- NL-to-SQL reporting uses a read-only database role with restricted search_path
- Generated SQL is validated before execution (no DDL, no writes, no function calls)

---

## 15. Migration Strategy

### 15.1 Phase 0: Foundation (no user-facing changes)

1. Add `bpe-core` and `bpe-server` crates to workspace
2. Implement `bpe.` schema DDL in `bpe-core/src/db/migrations.rs`
3. Implement JWT validation (reuse pattern from goal-rag `auth.rs`)
4. Implement PgPool (copy pattern from goal-rag `postgres/pool.rs`)
5. Run `bpe-server` binary on port 8090 with `/bpe/api/health` endpoint
6. Deploy to VM as `bpe-server.service` (systemd unit, separate from goal-rag)
7. Configure nginx to proxy `/bpe/*` to port 8090

### 15.2 Coexistence with goal-rag

- Both services connect to the same `goalrag` PostgreSQL database
- BPE uses `bpe` schema, goal-rag uses `api` schema and `public` schema
- BPE references `api.organizations`, `api.users` via foreign keys
- No conflicts: BPE never writes to `api.*` tables (reads only for user/org lookups)
- Shared connection pool config (same env vars: `POSTGRES_HOST`, etc.)

### 15.3 systemd Unit

```ini
# /etc/systemd/system/bpe-server.service
[Unit]
Description=Business Process Engine Server
After=network.target postgresql.service

[Service]
Type=simple
User=deploy
ExecStart=/usr/local/bin/bpe-server
Environment=BPE_PORT=8090
Environment=POSTGRES_HOST=localhost
Environment=POSTGRES_PORT=5432
Environment=POSTGRES_DATABASE=goalrag
Environment=POSTGRES_USER=postgres
Environment=POSTGRES_PASSWORD=<password>
Environment=POSTGREST_JWT_SECRET=YOUR_JWT_SECRET
Environment=BPE_ENCRYPTION_KEY=<32-byte-hex>
Environment=GOALRAG_URL=http://localhost:8080
Environment=GOOGLE_APPLICATION_CREDENTIALS=/path/to/credentials.json
Environment=GCP_PROJECT_ID=<project>
Environment=GCP_LOCATION=us-central1
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### 15.4 nginx Configuration Addition

```nginx
# Add to existing server block
location /bpe/ {
    proxy_pass http://127.0.0.1:8090;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

### 15.5 Build and Deploy

```bash
# Build
cargo build --release -p bpe-server

# Deploy
ssh app-server "cd /home/deploy/marshal && git pull && source ~/.cargo/env && cargo build --release -p bpe-server"
ssh app-server "sudo systemctl stop bpe-server.service && sudo cp /home/deploy/marshal/target/release/bpe-server /usr/local/bin/ && sudo systemctl start bpe-server.service"

# DB migration (auto-runs on startup, but can also run manually)
ssh app-server "sudo -u postgres psql -d goalrag -f /home/deploy/marshal/crates/bpe-core/migrations/001_initial.sql"
```

---

## 16. Build Order

### Phase 1: Core Infrastructure (Week 1-2)

**Dependencies: None**

1. **Crate scaffolding** — Create `bpe-core/` and `bpe-server/` crate directories, Cargo.toml files, add to workspace
2. **Database module** — `bpe-core/src/db/pool.rs` (PgPool), `migrations.rs` (CREATE SCHEMA + all tables)
3. **Auth module** — `bpe-core/src/auth.rs` (JWT validation middleware, Claims extraction)
4. **Error types** — `bpe-core/src/error.rs` (BpeError enum with IntoResponse)
5. **Config** — `bpe-core/src/config.rs` (BpeConfig from env vars)
6. **Server skeleton** — `bpe-server/src/main.rs` with Axum app, health endpoint, CORS
7. **Deploy pipeline** — systemd unit, nginx config, first deployment

**Deliverable**: `bpe-server` running on VM, `/bpe/api/health` returns OK, connects to DB, validates JWTs.

### Phase 2: Entity System (Week 2-3)

**Dependencies: Phase 1**

1. **Entity types** — `bpe-core/src/entity/registry.rs` (seed system types, CRUD)
2. **Entity CRUD** — `bpe-core/src/entity/models.rs`, entity routes
3. **Attribute validation** — `bpe-core/src/entity/attributes.rs` (FieldType validation)
4. **Relationships** — `bpe-core/src/entity/relationships.rs`
5. **Interactions** — `bpe-core/src/entity/interactions.rs`
6. **Entity API routes** — `bpe-server/src/routes/entities.rs`, `entity_types.rs`

**Deliverable**: Full entity CRUD API working. Can create employees, suppliers, set up relationships.

### Phase 3: Audit System (Week 3)

**Dependencies: Phase 1**

1. **Audit logger** — `bpe-core/src/audit/logger.rs` (insert events, partition management)
2. **Audit query** — `bpe-core/src/audit/query.rs` (filter, paginate, search)
3. **Audit routes** — `bpe-server/src/routes/audit.rs`
4. **Integrate with entity operations** — add audit logging to all entity CRUD

**Deliverable**: All entity changes produce audit events. Audit API queryable.

### Phase 4: Workflow Engine (Week 3-5)

**Dependencies: Phase 1, Phase 2**

1. **LLM client** — `bpe-core/src/llm/gemini.rs` (Gemini client for decomposition)
2. **Prompt templates** — `bpe-core/src/llm/prompts.rs`
3. **Goal decomposer** — `bpe-core/src/workflow/decomposer.rs` (RAG query + Gemini decomposition)
4. **Step state machine** — `bpe-core/src/workflow/state_machine.rs`
5. **Workflow engine** — `bpe-core/src/workflow/engine.rs` (lifecycle management)
6. **Step scheduler** — `bpe-core/src/workflow/scheduler.rs` (dependency DAG)
7. **Background ticker** — `tokio::spawn` in server main, 30s interval
8. **Workflow routes** — `bpe-server/src/routes/workflows.rs`
9. **Workflow definitions** — CRUD for reusable templates

**Deliverable**: Can decompose a goal into steps via LLM+RAG, confirm, execute manual steps, complete workflows.

### Phase 5: Approval Engine (Week 5-6)

**Dependencies: Phase 4**

1. **Rule evaluator** — `bpe-core/src/approval/rules.rs` (condition DSL evaluation)
2. **Approval engine** — `bpe-core/src/approval/engine.rs` (create requests, decide, delegate)
3. **Escalation** — `bpe-core/src/approval/escalation.rs` (timeout detection, escalation)
4. **Notification system** — `bpe.notifications` table, API routes
5. **Approval routes** — `bpe-server/src/routes/approvals.rs`
6. **Wire into workflow engine** — approval steps trigger approval requests

**Deliverable**: Workflow steps can require approval. Approvers see pending queue. Timeouts escalate.

### Phase 6: Integration Framework (Week 6-8)

**Dependencies: Phase 4**

1. **Adapter trait** — `bpe-core/src/integration/adapter.rs`
2. **Credential manager** — `bpe-core/src/integration/credentials.rs` (AES-256-GCM encryption)
3. **Adapter registry** — `bpe-core/src/integration/registry.rs`
4. **Email adapter** — `bpe-core/src/integration/adapters/email.rs`
5. **Webhook adapter** — `bpe-core/src/integration/adapters/webhook.rs`
6. **Xero adapter** — `bpe-core/src/integration/adapters/xero.rs` (OAuth2 flow, invoice API)
7. **Integration routes** — `bpe-server/src/routes/integrations.rs`
8. **Wire into workflow engine** — integration steps call adapters

**Deliverable**: Workflows can send emails and create Xero invoices automatically. Credentials stored encrypted.

### Phase 7: Knowledge Learning (Week 8-9)

**Dependencies: Phase 4**

1. **Sequence learner** — `bpe-core/src/knowledge/learner.rs`
2. **Sequence suggester** — `bpe-core/src/knowledge/suggester.rs`
3. **Wire into decomposer** — suggest learned sequences before LLM decomposition
4. **Wire into workflow completion** — learn from completed executions
5. **Feedback API** — record accept/modify/reject on suggestions

**Deliverable**: System learns from completed workflows and suggests sequences for future similar tasks.

### Phase 8: Reporting (Week 9-10)

**Dependencies: Phase 1, Phase 2**

1. **NL-to-SQL engine** — `bpe-core/src/reporting/engine.rs`
2. **Report templates** — `bpe-core/src/reporting/templates.rs` (seed defaults)
3. **Export module** — `bpe-core/src/reporting/export.rs` (CSV, Excel)
4. **Report routes** — `bpe-server/src/routes/reports.rs`

**Deliverable**: Users can ask natural language questions and get formatted data. Pre-built report templates available.

### Phase 9: Frontend (Week 5-10, parallel with backend phases)

**Dependencies: Corresponding backend phases**

1. **Phase 5 parallel**: Process Dashboard, Entity Manager pages
2. **Phase 6 parallel**: Workflow Builder, Workflow Execution pages
3. **Phase 7 parallel**: Approval Queue, Approval Rule Manager
4. **Phase 8 parallel**: Integration Manager
5. **Phase 9 parallel**: Report Builder, Audit Log, Notification Center

### Phase 10: Hardening (Week 10-11)

1. **Rate limiting** — per-org request limits on BPE API
2. **RBAC** — BPE-specific role table and middleware
3. **Monitoring** — Prometheus metrics endpoint
4. **Load testing** — verify concurrent workflow execution
5. **DocuSign and QuickBooks adapters** — remaining integrations
6. **PDF export** — reporting PDF generation

---

## Appendix A: Gemini Decomposition Prompt

```
SYSTEM:
You are a business process decomposition engine. Given a task description and
organizational context (SOPs, policies, laws), break the task into ordered
executable steps.

For each step, provide:
- name: short action description
- description: detailed instructions
- type: one of [manual, automated, approval, integration, llm_action]
- estimated_duration_minutes: integer
- dependencies: array of step indices (0-based) that must complete first
- integration_type: if type is "integration", specify which system (email, xero, docusign, webhook, null)
- requires_approval: boolean

Rules:
- Steps must be in logical order
- Parallel steps should share the same dependency set
- Include approval steps for financial transactions, legal actions, personnel changes
- Include integration steps for external system interactions (payments, notifications, document signing)
- If SOPs are provided, follow them exactly
- If no SOPs match, generate reasonable steps based on common business practices and applicable laws

Respond in JSON format:
{
  "steps": [...],
  "integrations_needed": ["email", "xero"],
  "notes": "any caveats or assumptions"
}

USER:
Task: {prompt}

Organization context (from knowledge base):
{rag_context}

Previously learned sequences for similar tasks:
{learned_sequences}

Entity context:
{entity_context}
```

## Appendix B: Default Report Templates

| Template | Category | SQL Summary | Parameters |
|---|---|---|---|
| Employees by Grade | HR | `SELECT * FROM bpe.entities JOIN bpe.entity_types WHERE name='employee' AND attributes->>'grade' = $2` | grade |
| Pending Approvals Summary | Operations | `SELECT rule.name, COUNT(*) FROM bpe.approval_requests WHERE status='pending' GROUP BY rule_id` | (none) |
| Workflow Completion Times | Operations | `SELECT title, EXTRACT(EPOCH FROM completed_at - started_at)/60 FROM bpe.workflow_executions WHERE status='completed'` | date_from, date_to |
| Overdue Steps | Operations | `SELECT * FROM bpe.workflow_steps WHERE status IN ('ready','in_progress') AND estimated_completion < now()` | (none) |
| Integration Error Log | IT | `SELECT * FROM bpe.audit_events WHERE event_type='integration.failed' ORDER BY created_at DESC` | date_from, date_to |
| Entity Interaction History | General | `SELECT * FROM bpe.entity_interactions WHERE entity_id = $2 ORDER BY created_at DESC` | entity_id |
| Approval Turnaround Times | Compliance | `SELECT rule.name, AVG(EXTRACT(EPOCH FROM resolved_at - created_at)/3600) FROM bpe.approval_requests WHERE status IN ('approved','rejected')` | date_from, date_to |

## Appendix C: Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `BPE_PORT` | No | `8090` | HTTP server port |
| `BPE_HOST` | No | `0.0.0.0` | Bind address |
| `POSTGRES_HOST` | No | `localhost` | DB host (shared with goal-rag) |
| `POSTGRES_PORT` | No | `5432` | DB port |
| `POSTGRES_DATABASE` | No | `goalrag` | DB name |
| `POSTGRES_USER` | No | `postgres` | DB user |
| `POSTGRES_PASSWORD` | Yes | (empty) | DB password |
| `POSTGREST_JWT_SECRET` | No | fallback key | JWT signing secret (same as goal-rag) |
| `BPE_ENCRYPTION_KEY` | Yes (prod) | dev default | 32-byte hex key for credential encryption |
| `GOALRAG_URL` | No | `http://localhost:8080` | URL to goal-rag for RAG queries |
| `GOOGLE_APPLICATION_CREDENTIALS` | Yes | - | GCP service account JSON path |
| `GCP_PROJECT_ID` | Yes | - | GCP project for Gemini |
| `GCP_LOCATION` | No | `us-central1` | GCP region |
| `BPE_GEMINI_MODEL` | No | `gemini-2.5-pro` | Gemini model name |
| `BPE_TICK_INTERVAL_SECS` | No | `30` | Background ticker interval |
| `BPE_LOG_LEVEL` | No | `info` | Log level (trace/debug/info/warn/error) |
