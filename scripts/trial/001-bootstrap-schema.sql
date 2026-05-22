-- Marshal Trial — Database Bootstrap
-- Run against: marshal_trial database on marshal-trial-db instance
-- This script creates the base schema, trial extensions, and seed data.

-- =============================================================
-- 1. Enable extensions
-- =============================================================
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "vector";  -- pgvector

-- =============================================================
-- 2. Core schemas
-- =============================================================
CREATE SCHEMA IF NOT EXISTS api;
CREATE SCHEMA IF NOT EXISTS bpe;
CREATE SCHEMA IF NOT EXISTS timekeeping;
CREATE SCHEMA IF NOT EXISTS trial;

-- =============================================================
-- 3. api schema — core tables
-- =============================================================

-- Organizations
CREATE TABLE IF NOT EXISTS api.organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    display_name TEXT,
    description TEXT,
    logo_url TEXT,
    created_by UUID,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    -- Trial extensions
    trial_started_at TIMESTAMPTZ,
    trial_expires_at TIMESTAMPTZ,
    trial_status VARCHAR(20) DEFAULT 'active'
        CHECK (trial_status IN ('active', 'expired', 'suspended', 'converted')),
    email_domain VARCHAR(255),
    domain_verified BOOLEAN DEFAULT false,
    logo_hash VARCHAR(64),
    eula_accepted_at TIMESTAMPTZ,
    eula_accepted_by UUID
);
CREATE INDEX IF NOT EXISTS idx_orgs_email_domain ON api.organizations(email_domain);
CREATE INDEX IF NOT EXISTS idx_orgs_trial_status ON api.organizations(trial_status);
CREATE INDEX IF NOT EXISTS idx_orgs_logo_hash ON api.organizations(logo_hash);

-- Users
CREATE TABLE IF NOT EXISTS api.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    username TEXT UNIQUE,
    email TEXT UNIQUE NOT NULL,
    mobile_phone TEXT,
    avatar_url TEXT,
    badge_number TEXT,
    title TEXT,
    level TEXT,
    manager_id UUID REFERENCES api.users(id),
    password_hash TEXT,
    is_deleted BOOLEAN NOT NULL DEFAULT false,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_users_org ON api.users(organization_id);
CREATE INDEX IF NOT EXISTS idx_users_email ON api.users(email);
CREATE INDEX IF NOT EXISTS idx_users_is_deleted ON api.users(is_deleted);

-- Add foreign key for eula_accepted_by now that users table exists
ALTER TABLE api.organizations
    ADD CONSTRAINT fk_orgs_eula_accepted_by
    FOREIGN KEY (eula_accepted_by) REFERENCES api.users(id)
    NOT VALID;

-- Roles
CREATE TABLE IF NOT EXISTS api.roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(100) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);

-- User roles
CREATE TABLE IF NOT EXISTS api.user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES api.users(id),
    role TEXT NOT NULL,
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(user_id, role, organization_id)
);
CREATE INDEX IF NOT EXISTS idx_user_roles_user ON api.user_roles(user_id, organization_id);

-- Permission enums (as check constraints since PostgREST works better with text)
CREATE TABLE IF NOT EXISTS api.role_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id UUID NOT NULL REFERENCES api.roles(id) ON DELETE CASCADE,
    feature TEXT NOT NULL,
    action TEXT NOT NULL,
    organization_id UUID REFERENCES api.organizations(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(role_id, feature, action)
);

-- Groups
CREATE TABLE IF NOT EXISTS api.groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name VARCHAR(200) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);

-- User groups
CREATE TABLE IF NOT EXISTS api.user_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES api.users(id),
    group_id UUID NOT NULL REFERENCES api.groups(id) ON DELETE CASCADE,
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(user_id, group_id, organization_id)
);

-- Group permissions
CREATE TABLE IF NOT EXISTS api.group_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id UUID NOT NULL REFERENCES api.groups(id) ON DELETE CASCADE,
    feature TEXT NOT NULL,
    action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(group_id, feature, action)
);

-- Document ACLs
CREATE TABLE IF NOT EXISTS api.document_acls (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL,
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    grant_type TEXT NOT NULL,
    grant_id UUID NOT NULL,
    action TEXT NOT NULL,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(document_id, grant_type, grant_id, action)
);
CREATE INDEX IF NOT EXISTS idx_doc_acls_doc ON api.document_acls(document_id);
CREATE INDEX IF NOT EXISTS idx_doc_acls_grant ON api.document_acls(grant_type, grant_id);

-- Tasks
CREATE TABLE IF NOT EXISTS api.tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    title TEXT NOT NULL,
    description TEXT,
    status TEXT DEFAULT 'pending',
    priority TEXT DEFAULT 'medium',
    assigned_to UUID REFERENCES api.users(id),
    goal_id UUID,
    due_date TIMESTAMPTZ,
    created_by UUID REFERENCES api.users(id),
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_tasks_org ON api.tasks(organization_id);

-- Goals
CREATE TABLE IF NOT EXISTS api.goals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    title TEXT NOT NULL,
    description TEXT,
    status TEXT DEFAULT 'active',
    parent_goal_id UUID REFERENCES api.goals(id),
    created_by UUID REFERENCES api.users(id),
    due_date TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_goals_org ON api.goals(organization_id);

-- Conversations & Messages (for Ask Marshal)
CREATE TABLE IF NOT EXISTS api.conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    user_id UUID NOT NULL REFERENCES api.users(id),
    title TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE IF NOT EXISTS api.messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES api.conversations(id) ON DELETE CASCADE,
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    role TEXT NOT NULL DEFAULT 'user',
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- =============================================================
-- 4. RAG tables (used by goal-rag)
-- =============================================================

CREATE TABLE IF NOT EXISTS rag_file_registry (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    file_type TEXT,
    file_size BIGINT,
    document_id UUID,
    status TEXT DEFAULT 'pending',
    error_message TEXT,
    chunk_count INTEGER DEFAULT 0,
    last_processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(organization_id, filename)
);
CREATE INDEX IF NOT EXISTS idx_rag_files_org_id ON rag_file_registry(organization_id);

CREATE TABLE IF NOT EXISTS rag_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL,
    organization_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding vector(768),
    content_tsv TSVECTOR,
    metadata JSONB DEFAULT '{}',
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_org ON rag_chunks(organization_id);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_doc ON rag_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_tsv ON rag_chunks USING GIN(content_tsv);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_embedding ON rag_chunks USING hnsw (embedding vector_cosine_ops);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_not_archived ON rag_chunks(archived_at) WHERE archived_at IS NULL;

-- Entity embeddings
CREATE TABLE IF NOT EXISTS entity_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    content TEXT,
    embedding vector(768),
    status TEXT DEFAULT 'active',
    priority TEXT,
    sentiment REAL,
    source_tool TEXT,
    embedded_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(entity_type, entity_id)
);
CREATE INDEX IF NOT EXISTS idx_entity_emb_org ON entity_embeddings(organization_id);
CREATE INDEX IF NOT EXISTS idx_entity_emb_hnsw ON entity_embeddings USING hnsw (embedding vector_cosine_ops);

-- =============================================================
-- 5. Trial schema tables
-- =============================================================

-- Resource quotas per org
CREATE TABLE IF NOT EXISTS trial.org_quotas (
    organization_id UUID PRIMARY KEY REFERENCES api.organizations(id),
    max_users INTEGER NOT NULL DEFAULT 25,
    max_storage_bytes BIGINT NOT NULL DEFAULT 10737418240,  -- 10 GB
    max_documents INTEGER NOT NULL DEFAULT 200,
    current_users INTEGER NOT NULL DEFAULT 0,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_documents INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Join requests for existing orgs
CREATE TABLE IF NOT EXISTS trial.join_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    requester_email VARCHAR(255) NOT NULL,
    requester_first_name VARCHAR(255) NOT NULL,
    requester_last_name VARCHAR(255) NOT NULL,
    requester_password_hash TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected', 'expired')),
    reviewed_by UUID REFERENCES api.users(id),
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '7 days')
);
CREATE INDEX IF NOT EXISTS idx_join_req_org ON trial.join_requests(organization_id, status);
CREATE INDEX IF NOT EXISTS idx_join_req_email ON trial.join_requests(requester_email);

-- EULA versions
CREATE TABLE IF NOT EXISTS trial.eula_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version VARCHAR(20) NOT NULL UNIQUE,
    content TEXT NOT NULL,
    effective_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- EULA acceptance audit trail
CREATE TABLE IF NOT EXISTS trial.eula_acceptances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES api.users(id),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    eula_version_id UUID NOT NULL REFERENCES trial.eula_versions(id),
    ip_address INET,
    user_agent TEXT,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_eula_acc_user ON trial.eula_acceptances(user_id);

-- Signup rate limiting
CREATE TABLE IF NOT EXISTS trial.signup_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ip_address INET NOT NULL,
    email VARCHAR(255),
    email_domain VARCHAR(255),
    outcome VARCHAR(20) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_signup_ip ON trial.signup_attempts(ip_address, created_at);
CREATE INDEX IF NOT EXISTS idx_signup_domain ON trial.signup_attempts(email_domain, created_at);

-- Email verification tokens
CREATE TABLE IF NOT EXISTS trial.email_verifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL,
    token VARCHAR(128) NOT NULL UNIQUE,
    purpose VARCHAR(30) NOT NULL DEFAULT 'signup',
    verified_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '24 hours'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Throwaway domain blocklist
CREATE TABLE IF NOT EXISTS trial.blocked_domains (
    domain VARCHAR(255) PRIMARY KEY,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Conversion hooks (placeholder)
CREATE TABLE IF NOT EXISTS trial.subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    plan VARCHAR(50) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    stripe_customer_id VARCHAR(255),
    stripe_subscription_id VARCHAR(255),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ
);

-- =============================================================
-- 6. Seed data
-- =============================================================

-- EULA v1.0
INSERT INTO trial.eula_versions (version, content, effective_at) VALUES (
    '1.0',
    E'MARSHAL TRIAL — END USER LICENSE AGREEMENT\n\nEffective Date: May 20, 2026\n\nBy creating an account on the Marshal Trial platform ("Platform"), you ("User") agree to the following terms:\n\n1. TRIAL DISCLAIMER\nThis is a trial system provided on an "as is" basis with no guaranteed uptime, availability, or service level agreement (SLA). The Platform may be modified, suspended, or terminated at any time without notice.\n\n2. NO SERVICE OR SUPPORT EXPECTATION\nMarshal provides this trial on a best-effort basis. There is no obligation to provide technical support, maintenance, or updates during the trial period.\n\n3. PROHIBITED CONTENT AND USE\nUsers shall not upload, store, or distribute:\n  (a) Illegal content of any kind\n  (b) Stolen, pirated, or misappropriated materials\n  (c) Indecent, obscene, or offensive materials\n  (d) Materials for which the User does not hold distribution rights\n\nBy uploading any content, User attests that they have the legal right to distribute said content.\n\n4. ACCEPTABLE USE\nUsers shall not use the Platform for illegal activities, harassment, unauthorized access to systems, or any purpose that violates applicable law.\n\n5. MONITORING DISCLOSURE\nMarshal reserves the right to monitor platform activity and use anonymized analytics to improve the service for all users. Activity logs may be reviewed for abuse prevention and platform optimization.\n\n6. DATA RETENTION\nAll data will be permanently deleted 30 days after trial expiration unless the account is converted to a paid plan. Users are responsible for exporting their data before this deadline.\n\n7. TERMINATION\nMarshal may suspend or terminate any trial account at its sole discretion, with or without cause, and with or without notice.\n\n8. LIMITATION OF LIABILITY\nIn no event shall Marshal be liable for any indirect, incidental, special, consequential, or punitive damages arising from the use of this Platform.\n\n9. AGREEMENT\nBy clicking "I Agree" or creating an account, User acknowledges that they have read, understood, and agree to be bound by this Agreement.',
    now()
) ON CONFLICT (version) DO NOTHING;

-- Seed blocked domains (top disposable email providers)
INSERT INTO trial.blocked_domains (domain, reason) VALUES
    ('mailinator.com', 'Disposable email'),
    ('guerrillamail.com', 'Disposable email'),
    ('tempmail.com', 'Disposable email'),
    ('throwaway.email', 'Disposable email'),
    ('yopmail.com', 'Disposable email'),
    ('sharklasers.com', 'Disposable email'),
    ('guerrillamailblock.com', 'Disposable email'),
    ('grr.la', 'Disposable email'),
    ('dispostable.com', 'Disposable email'),
    ('maildrop.cc', 'Disposable email'),
    ('10minutemail.com', 'Disposable email'),
    ('trashmail.com', 'Disposable email'),
    ('fakeinbox.com', 'Disposable email'),
    ('temp-mail.org', 'Disposable email'),
    ('emailondeck.com', 'Disposable email'),
    ('getairmail.com', 'Disposable email'),
    ('mohmal.com', 'Disposable email'),
    ('burnermail.io', 'Disposable email'),
    ('tempail.com', 'Disposable email'),
    ('mintemail.com', 'Disposable email')
ON CONFLICT (domain) DO NOTHING;

-- =============================================================
-- 7. Verify
-- =============================================================
SELECT 'Bootstrap complete' AS status,
       (SELECT count(*) FROM information_schema.tables WHERE table_schema IN ('api', 'trial')) AS table_count,
       (SELECT count(*) FROM trial.blocked_domains) AS blocked_domains,
       (SELECT version FROM trial.eula_versions LIMIT 1) AS eula_version;
