use crate::error::BpeError;

/// Run timekeeping schema migrations.
/// Safe to call on every startup (uses IF NOT EXISTS / ON CONFLICT DO NOTHING).
pub async fn run_timekeeping_migrations(client: &deadpool_postgres::Client) -> Result<(), BpeError> {
    tracing::info!("Running timekeeping schema migrations...");

    client.batch_execute(TIMEKEEPING_MIGRATION_SQL).await.map_err(|e| {
        BpeError::Database(format!("Timekeeping migration failed: {e}"))
    })?;

    tracing::info!("Timekeeping schema migrations complete");
    Ok(())
}

const TIMEKEEPING_MIGRATION_SQL: &str = r#"
CREATE SCHEMA IF NOT EXISTS timekeeping;

-- 1. Stations (must precede employees FK)
CREATE TABLE IF NOT EXISTS timekeeping.stations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    name            VARCHAR(100) NOT NULL,
    station_number  INTEGER NOT NULL,
    address         TEXT,
    min_staffing    INTEGER NOT NULL DEFAULT 3,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, station_number)
);

-- 2. Employees
CREATE TABLE IF NOT EXISTS timekeeping.employees (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    user_id         UUID REFERENCES api.users(id),
    first_name      VARCHAR(100) NOT NULL,
    last_name       VARCHAR(100) NOT NULL,
    rank            VARCHAR(50)  NOT NULL,
    employee_number VARCHAR(50),
    shift_assignment CHAR(1),
    default_station_id UUID REFERENCES timekeeping.stations(id),
    phone1          VARCHAR(30),
    phone2          VARCHAR(30),
    address_line1   VARCHAR(200),
    city            VARCHAR(100),
    state           VARCHAR(2),
    zip             VARCHAR(10),
    hire_date       DATE,
    status          VARCHAR(20)  NOT NULL DEFAULT 'active',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_tk_employees_org ON timekeeping.employees(organization_id);
CREATE INDEX IF NOT EXISTS idx_tk_employees_shift ON timekeeping.employees(organization_id, shift_assignment);
CREATE INDEX IF NOT EXISTS idx_tk_employees_status ON timekeeping.employees(organization_id, status);

-- 3. Kelly Schedule Configuration
CREATE TABLE IF NOT EXISTS timekeeping.kelly_schedule_config (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    epoch_date      DATE NOT NULL,
    cycle_length    INTEGER NOT NULL DEFAULT 3,
    shift_labels    TEXT[] NOT NULL DEFAULT '{A,B,C}',
    rotation_pattern INTEGER[] NOT NULL DEFAULT '{0,1,2}',
    shift_start_time TIME NOT NULL DEFAULT '08:00:00',
    shift_duration_hours INTEGER NOT NULL DEFAULT 24,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tk_kelly_active
    ON timekeeping.kelly_schedule_config(organization_id) WHERE is_active = true;

-- 4. Pay Codes (configurable per org)
CREATE TABLE IF NOT EXISTS timekeeping.pay_codes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    code            VARCHAR(50) NOT NULL,
    display_name    VARCHAR(100) NOT NULL,
    category        VARCHAR(20) NOT NULL,
    multiplier      NUMERIC(4,2) NOT NULL DEFAULT 1.0,
    is_overtime     BOOLEAN NOT NULL DEFAULT false,
    counts_toward_flsa BOOLEAN NOT NULL DEFAULT true,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, code)
);

-- 5. Shift Roster (daily staffing)
CREATE TABLE IF NOT EXISTS timekeeping.shift_roster (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    roster_date     DATE NOT NULL,
    shift_label     CHAR(1) NOT NULL,
    duty_chief_id   UUID REFERENCES timekeeping.employees(id),
    notes           TEXT,
    is_auto_generated BOOLEAN NOT NULL DEFAULT true,
    is_locked       BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, roster_date)
);
CREATE INDEX IF NOT EXISTS idx_tk_roster_date ON timekeeping.shift_roster(organization_id, roster_date);

-- 6. Roster Assignments
CREATE TABLE IF NOT EXISTS timekeeping.roster_assignments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    roster_id       UUID NOT NULL REFERENCES timekeeping.shift_roster(id) ON DELETE CASCADE,
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    station_id      UUID REFERENCES timekeeping.stations(id),
    assignment_type VARCHAR(20) NOT NULL DEFAULT 'regular',
    is_cism_coverage BOOLEAN NOT NULL DEFAULT false,
    is_24hr         BOOLEAN NOT NULL DEFAULT false,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(roster_id, employee_id)
);
-- Migration: add is_24hr if missing (safe for existing deployments)
ALTER TABLE timekeeping.roster_assignments ADD COLUMN IF NOT EXISTS is_24hr BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX IF NOT EXISTS idx_tk_roster_assign_roster ON timekeeping.roster_assignments(roster_id);
CREATE INDEX IF NOT EXISTS idx_tk_roster_assign_emp ON timekeeping.roster_assignments(employee_id);

-- 7. Absences
CREATE TABLE IF NOT EXISTS timekeeping.absences (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    roster_id       UUID REFERENCES timekeeping.shift_roster(id),
    absence_date    DATE NOT NULL,
    absence_type    VARCHAR(30) NOT NULL,
    notes           TEXT,
    approved_by     UUID REFERENCES api.users(id),
    approved_at     TIMESTAMPTZ,
    status          VARCHAR(20) NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(employee_id, absence_date)
);
CREATE INDEX IF NOT EXISTS idx_tk_absences_emp ON timekeeping.absences(employee_id, absence_date);
CREATE INDEX IF NOT EXISTS idx_tk_absences_date ON timekeeping.absences(organization_id, absence_date);

-- 8. Time Entries
CREATE TABLE IF NOT EXISTS timekeeping.time_entries (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    pay_code_id     UUID NOT NULL REFERENCES timekeeping.pay_codes(id),
    work_date       DATE NOT NULL,
    start_time      TIME,
    hours           NUMERIC(5,2) NOT NULL CHECK (hours > 0 AND hours <= 48),
    notes           TEXT,
    entered_by      UUID NOT NULL REFERENCES api.users(id),
    status          VARCHAR(20) NOT NULL DEFAULT 'draft',
    submitted_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_tk_time_entries_emp_date ON timekeeping.time_entries(employee_id, work_date);
CREATE INDEX IF NOT EXISTS idx_tk_time_entries_org_date ON timekeeping.time_entries(organization_id, work_date);
CREATE INDEX IF NOT EXISTS idx_tk_time_entries_status ON timekeeping.time_entries(organization_id, status);

-- 9. Timecard Periods
CREATE TABLE IF NOT EXISTS timekeeping.timecard_periods (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    period_start    DATE NOT NULL,
    period_end      DATE NOT NULL,
    flsa_cycle_start DATE,
    status          VARCHAR(20) NOT NULL DEFAULT 'open',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, period_start),
    CHECK(period_end > period_start)
);

-- 10. Timecard Certifications
CREATE TABLE IF NOT EXISTS timekeeping.timecard_certifications (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    period_id       UUID NOT NULL REFERENCES timekeeping.timecard_periods(id),
    certified_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    ip_address      INET,
    signature_text  VARCHAR(200),
    UNIQUE(employee_id, period_id)
);

-- 11. Timecard Approvals
CREATE TABLE IF NOT EXISTS timekeeping.timecard_approvals (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    period_id       UUID NOT NULL REFERENCES timekeeping.timecard_periods(id),
    supervisor_id   UUID NOT NULL REFERENCES api.users(id),
    decision        VARCHAR(20) NOT NULL,
    notes           TEXT,
    decided_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(employee_id, period_id, supervisor_id)
);

-- 12. Validation Flags
CREATE TABLE IF NOT EXISTS timekeeping.validation_flags (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    time_entry_id   UUID REFERENCES timekeeping.time_entries(id) ON DELETE CASCADE,
    roster_id       UUID REFERENCES timekeeping.shift_roster(id),
    flag_type       VARCHAR(50) NOT NULL,
    severity        VARCHAR(10) NOT NULL DEFAULT 'warning',
    message         TEXT NOT NULL,
    is_resolved     BOOLEAN NOT NULL DEFAULT false,
    resolved_by     UUID REFERENCES api.users(id),
    resolved_at     TIMESTAMPTZ,
    resolution_note TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_tk_flags_org ON timekeeping.validation_flags(organization_id, is_resolved);
CREATE INDEX IF NOT EXISTS idx_tk_flags_emp ON timekeeping.validation_flags(employee_id);
-- Migration: add flag_date for date-based filtering
ALTER TABLE timekeeping.validation_flags ADD COLUMN IF NOT EXISTS flag_date DATE;
UPDATE timekeeping.validation_flags SET flag_date = created_at::date WHERE flag_date IS NULL;
CREATE INDEX IF NOT EXISTS idx_tk_flags_date ON timekeeping.validation_flags(organization_id, flag_date);

-- 13. Leave Balances
CREATE TABLE IF NOT EXISTS timekeeping.leave_balances (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    employee_id     UUID NOT NULL REFERENCES timekeeping.employees(id),
    leave_type      VARCHAR(30) NOT NULL,
    balance_hours   NUMERIC(7,2) NOT NULL DEFAULT 0,
    accrual_rate    NUMERIC(5,2) NOT NULL DEFAULT 0,
    max_balance     NUMERIC(7,2),
    year            INTEGER NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(employee_id, leave_type, year)
);

-- 14. Audit Trail
CREATE TABLE IF NOT EXISTS timekeeping.audit_trail (
    id              BIGSERIAL PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES api.organizations(id),
    actor_user_id   UUID REFERENCES api.users(id),
    actor_name      TEXT,
    employee_id     UUID REFERENCES timekeeping.employees(id),
    employee_name   TEXT,
    action          VARCHAR(50) NOT NULL,
    resource_type   VARCHAR(50) NOT NULL,
    resource_id     UUID,
    before_state    JSONB,
    after_state     JSONB,
    summary         TEXT NOT NULL,
    ip_address      INET,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_tk_audit_org ON timekeeping.audit_trail(organization_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tk_audit_emp ON timekeeping.audit_trail(employee_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tk_audit_action ON timekeeping.audit_trail(organization_id, action);
CREATE INDEX IF NOT EXISTS idx_tk_audit_resource ON timekeeping.audit_trail(resource_type, resource_id);

-- Seed default pay codes for all orgs
INSERT INTO timekeeping.pay_codes (organization_id, code, display_name, category, multiplier, is_overtime, counts_toward_flsa, sort_order)
SELECT o.id, v.code, v.display_name, v.category, v.multiplier::numeric, v.is_overtime, v.counts_toward_flsa, v.sort_order
FROM api.organizations o
CROSS JOIN (VALUES
    ('REG',            'Regular',           'work',  1.0,  false, true,  1),
    ('OT',             'Overtime',          'work',  1.5,  true,  true,  2),
    ('HOLIDAY',        'Holiday',           'work',  1.0,  false, true,  3),
    ('CALLBACK',       'Callback',          'work',  1.5,  true,  true,  4),
    ('TRAINING',       'Training',          'work',  1.0,  false, true,  5),
    ('LIGHT_DUTY',     'Light Duty',        'work',  1.0,  false, true,  6),
    ('RESERVE_SHIFT',  'Reserve Shift',     'work',  1.0,  false, true,  7),
    ('RESERVE_DRILL',  'Reserve Drills',    'work',  1.0,  false, true,  8),
    ('RESERVE_STRIKE', 'Reserve Strike',    'work',  1.0,  false, true,  9),
    ('RESERVE_TC',     'Reserve Timecard',  'work',  1.0,  false, true,  10),
    ('VACATION',       'Vacation',          'leave', 1.0,  false, false, 20),
    ('SICK',           'Sick Leave',        'leave', 1.0,  false, false, 21),
    ('COMP_TIME',      'Comp Time',         'leave', 1.0,  false, false, 22),
    ('BEREAVEMENT',    'Bereavement',       'leave', 1.0,  false, false, 23),
    ('FMLA',           'FMLA',             'leave', 1.0,  false, false, 24),
    ('PERSONAL',       'Personal',          'leave', 1.0,  false, false, 25),
    ('WORKER_COMP',    'Worker Comp',       'leave', 1.0,  false, false, 26),
    ('MILITARY',       'Military Leave',    'leave', 1.0,  false, false, 27)
) AS v(code, display_name, category, multiplier, is_overtime, counts_toward_flsa, sort_order)
ON CONFLICT (organization_id, code) DO NOTHING;
"#;
