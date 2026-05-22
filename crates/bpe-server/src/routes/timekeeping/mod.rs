pub mod audit;
pub mod employees;
pub mod stations;
pub mod kelly;
pub mod roster;
pub mod absences;
pub mod pay_codes;
pub mod time_entries;
pub mod timecards;
pub mod validation;
pub mod reports;
pub mod leave_balances;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use crate::AppState;

/// Build the timekeeping sub-router.
/// All routes are relative to /bpe/api/timekeeping/...
pub fn timekeeping_routes() -> Router<AppState> {
    Router::new()
        // Employees
        .route("/employees", get(employees::list))
        .route("/employees", post(employees::create))
        .route("/employees/:id", get(employees::get_one))
        .route("/employees/:id", put(employees::update))
        .route("/employees/:id", delete(employees::delete))
        .route("/employees/import", post(employees::import))
        // Stations
        .route("/stations", get(stations::list))
        .route("/stations", post(stations::create))
        .route("/stations/:id", put(stations::update))
        .route("/stations/:id", delete(stations::delete))
        // Kelly schedule
        .route("/kelly-config", get(kelly::get_config))
        .route("/kelly-config", put(kelly::upsert_config))
        .route("/kelly-schedule", get(kelly::compute_schedule))
        // Shift roster
        .route("/roster", get(roster::get_roster))
        .route("/roster/range", get(roster::get_range))
        .route("/roster/generate", post(roster::generate))
        .route("/roster/:id", put(roster::update_roster))
        .route("/roster/:id/lock", post(roster::lock))
        .route("/roster/:id/unlock", post(roster::unlock))
        .route("/roster/:id/assignments", get(roster::get_assignments))
        .route("/roster/:id/assignments", put(roster::update_assignments))
        // Absences
        .route("/absences", get(absences::list))
        .route("/absences", post(absences::create))
        .route("/absences/:id/approve", post(absences::approve))
        .route("/absences/:id", delete(absences::delete))
        // Pay codes
        .route("/pay-codes", get(pay_codes::list))
        .route("/pay-codes", post(pay_codes::create))
        .route("/pay-codes/:id", put(pay_codes::update))
        .route("/pay-codes/:id", delete(pay_codes::delete))
        // Time entries
        .route("/time-entries", get(time_entries::list))
        .route("/time-entries", post(time_entries::create))
        .route("/time-entries/batch", post(time_entries::batch_create))
        .route("/time-entries/:id", put(time_entries::update))
        .route("/time-entries/:id", delete(time_entries::delete))
        .route("/time-entries/:id/submit", post(time_entries::submit))
        // Timecard periods & approval
        .route("/periods", get(timecards::list_periods))
        .route("/periods", post(timecards::create_period))
        .route("/periods/:id/close", post(timecards::close_period))
        .route("/certify", post(timecards::certify))
        .route("/approvals/pending", get(timecards::pending_approvals))
        .route("/approvals/decide", post(timecards::decide))
        // Validation
        .route("/validate", post(validation::validate))
        .route("/flags", get(validation::list_flags))
        .route("/flags/:id/resolve", post(validation::resolve_flag))
        // Reports
        .route("/reports/hours", get(reports::hours_report))
        .route("/reports/overtime", get(reports::overtime_report))
        .route("/reports/flsa", get(reports::flsa_report))
        .route("/reports/staffing", get(reports::staffing_report))
        .route("/reports/payroll-export", get(reports::payroll_export))
        // Leave balances
        .route("/leave-balances", get(leave_balances::list))
        .route("/leave-balances", put(leave_balances::adjust))
        // Audit trail
        .route("/audit", get(audit::list_audit_trail))
        .route("/audit/summary", get(audit::audit_summary))
}
