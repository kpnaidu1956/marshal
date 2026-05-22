use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Employees ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Employee {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Option<Uuid>,
    pub first_name: String,
    pub last_name: String,
    pub rank: String,
    pub employee_number: Option<String>,
    pub shift_assignment: Option<String>,
    pub default_station_id: Option<Uuid>,
    pub phone1: Option<String>,
    pub phone2: Option<String>,
    pub address_line1: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub zip: Option<String>,
    pub hire_date: Option<NaiveDate>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEmployeeRequest {
    pub organization_id: String,
    pub first_name: String,
    pub last_name: String,
    pub rank: String,
    #[serde(default)]
    pub employee_number: Option<String>,
    #[serde(default)]
    pub shift_assignment: Option<String>,
    #[serde(default)]
    pub default_station_id: Option<Uuid>,
    #[serde(default)]
    pub phone1: Option<String>,
    #[serde(default)]
    pub phone2: Option<String>,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub zip: Option<String>,
    #[serde(default)]
    pub hire_date: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEmployeeRequest {
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub rank: Option<String>,
    #[serde(default)]
    pub employee_number: Option<String>,
    #[serde(default)]
    pub shift_assignment: Option<String>,
    #[serde(default)]
    pub default_station_id: Option<Uuid>,
    #[serde(default)]
    pub phone1: Option<String>,
    #[serde(default)]
    pub phone2: Option<String>,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub zip: Option<String>,
    #[serde(default)]
    pub hire_date: Option<NaiveDate>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportEmployeeEntry {
    pub first_name: String,
    pub last_name: String,
    pub rank: String,
    #[serde(default)]
    pub shift_assignment: Option<String>,
    #[serde(default)]
    pub phone1: Option<String>,
    #[serde(default)]
    pub phone2: Option<String>,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub zip: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportEmployeesRequest {
    pub organization_id: String,
    pub employees: Vec<ImportEmployeeEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ListEmployeesQuery {
    pub organization_id: String,
    #[serde(default)]
    pub shift: Option<String>,
    #[serde(default)]
    pub rank: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
}

// ── Stations ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub station_number: i32,
    pub address: Option<String>,
    pub min_staffing: i32,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateStationRequest {
    pub organization_id: String,
    pub name: String,
    pub station_number: i32,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default = "default_min_staffing")]
    pub min_staffing: i32,
}

fn default_min_staffing() -> i32 { 3 }

#[derive(Debug, Deserialize)]
pub struct UpdateStationRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub min_staffing: Option<i32>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

// ── Kelly Schedule ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KellyScheduleConfig {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub epoch_date: NaiveDate,
    pub cycle_length: i32,
    pub shift_labels: Vec<String>,
    pub rotation_pattern: Vec<i32>,
    pub shift_start_time: NaiveTime,
    pub shift_duration_hours: i32,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpsertKellyConfigRequest {
    pub organization_id: String,
    pub epoch_date: NaiveDate,
    #[serde(default = "default_cycle_length")]
    pub cycle_length: i32,
    #[serde(default = "default_shift_labels")]
    pub shift_labels: Vec<String>,
    #[serde(default = "default_rotation_pattern")]
    pub rotation_pattern: Vec<i32>,
    #[serde(default = "default_shift_start")]
    pub shift_start_time: NaiveTime,
    #[serde(default = "default_shift_duration")]
    pub shift_duration_hours: i32,
}

fn default_cycle_length() -> i32 { 3 }
fn default_shift_labels() -> Vec<String> { vec!["A".into(), "B".into(), "C".into()] }
fn default_rotation_pattern() -> Vec<i32> { vec![0, 1, 2] }
fn default_shift_start() -> NaiveTime { NaiveTime::from_hms_opt(8, 0, 0).unwrap() }
fn default_shift_duration() -> i32 { 24 }

#[derive(Debug, Serialize)]
pub struct KellyDayInfo {
    pub date: NaiveDate,
    pub on_duty_shift: String,
}

#[derive(Debug, Deserialize)]
pub struct KellyScheduleQuery {
    pub organization_id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

// ── Shift Roster ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftRoster {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub roster_date: NaiveDate,
    pub shift_label: String,
    pub duty_chief_id: Option<Uuid>,
    pub duty_chief_name: Option<String>,
    pub notes: Option<String>,
    pub is_auto_generated: bool,
    pub is_locked: bool,
    pub assignments: Vec<RosterAssignment>,
    pub absences: Vec<Absence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterAssignment {
    pub id: Uuid,
    pub roster_id: Uuid,
    pub employee_id: Uuid,
    pub employee_name: Option<String>,
    pub employee_rank: Option<String>,
    pub employee_shift: Option<String>,
    pub station_id: Option<Uuid>,
    pub station_name: Option<String>,
    pub assignment_type: String,
    pub is_cism_coverage: bool,
    pub is_24hr: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateRosterRequest {
    pub organization_id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRosterRequest {
    #[serde(default)]
    pub duty_chief_id: Option<Uuid>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAssignmentsRequest {
    pub assignments: Vec<AssignmentUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct AssignmentUpdate {
    pub employee_id: Uuid,
    pub station_id: Option<Uuid>,
    #[serde(default = "default_assignment_type")]
    pub assignment_type: String,
    #[serde(default)]
    pub is_cism_coverage: bool,
    #[serde(default)]
    pub is_24hr: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_assignment_type() -> String { "regular".into() }

#[derive(Debug, Deserialize)]
pub struct RosterQuery {
    pub organization_id: String,
    pub date: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct RosterRangeQuery {
    pub organization_id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

// ── Absences ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Absence {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub employee_id: Uuid,
    pub employee_name: Option<String>,
    pub roster_id: Option<Uuid>,
    pub absence_date: NaiveDate,
    pub absence_type: String,
    pub notes: Option<String>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAbsenceRequest {
    pub organization_id: String,
    pub employee_id: Uuid,
    pub absence_date: NaiveDate,
    pub absence_type: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AbsenceQuery {
    pub organization_id: String,
    #[serde(default)]
    pub start: Option<NaiveDate>,
    #[serde(default)]
    pub end: Option<NaiveDate>,
    #[serde(default)]
    pub employee_id: Option<Uuid>,
    #[serde(default)]
    pub absence_type: Option<String>,
}

// ── Pay Codes ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayCode {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub code: String,
    pub display_name: String,
    pub category: String,
    pub multiplier: f64,
    pub is_overtime: bool,
    pub counts_toward_flsa: bool,
    pub is_active: bool,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreatePayCodeRequest {
    pub organization_id: String,
    pub code: String,
    pub display_name: String,
    pub category: String,
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
    #[serde(default)]
    pub is_overtime: bool,
    #[serde(default = "default_true")]
    pub counts_toward_flsa: bool,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_multiplier() -> f64 { 1.0 }
fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct UpdatePayCodeRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub multiplier: Option<f64>,
    #[serde(default)]
    pub is_overtime: Option<bool>,
    #[serde(default)]
    pub counts_toward_flsa: Option<bool>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub sort_order: Option<i32>,
}

// ── Time Entries ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeEntry {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub employee_id: Uuid,
    pub employee_name: Option<String>,
    pub pay_code_id: Uuid,
    pub pay_code_name: Option<String>,
    pub pay_code: Option<String>,
    pub work_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub hours: f64,
    pub notes: Option<String>,
    pub entered_by: Uuid,
    pub status: String,
    pub submitted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTimeEntryRequest {
    pub organization_id: String,
    pub employee_id: Uuid,
    pub pay_code_id: Uuid,
    pub work_date: NaiveDate,
    #[serde(default)]
    pub start_time: Option<NaiveTime>,
    pub hours: f64,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTimeEntryRequest {
    #[serde(default)]
    pub pay_code_id: Option<Uuid>,
    #[serde(default)]
    pub work_date: Option<NaiveDate>,
    #[serde(default)]
    pub start_time: Option<NaiveTime>,
    #[serde(default)]
    pub hours: Option<f64>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimeEntryQuery {
    pub organization_id: String,
    #[serde(default)]
    pub employee_id: Option<Uuid>,
    #[serde(default)]
    pub start: Option<NaiveDate>,
    #[serde(default)]
    pub end: Option<NaiveDate>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct BatchTimeEntryRequest {
    pub organization_id: String,
    pub entries: Vec<CreateTimeEntryRequest>,
}

// ── Timecard Periods ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimecardPeriod {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub flsa_cycle_start: Option<NaiveDate>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePeriodRequest {
    pub organization_id: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    #[serde(default)]
    pub flsa_cycle_start: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct CertifyTimecardRequest {
    pub organization_id: String,
    pub employee_id: Uuid,
    pub period_id: Option<Uuid>,
    #[serde(default)]
    pub signature_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimecardDecisionRequest {
    pub organization_id: String,
    pub employee_id: Uuid,
    pub period_id: Uuid,
    pub decision: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimecardSummary {
    pub employee_id: Uuid,
    pub employee_name: String,
    pub period_id: Uuid,
    pub total_hours: f64,
    pub regular_hours: f64,
    pub overtime_hours: f64,
    pub leave_hours: f64,
    pub is_certified: bool,
    pub is_approved: bool,
    pub entries: Vec<TimeEntry>,
    pub flags: Vec<ValidationFlag>,
}

// ── Validation Flags ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFlag {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub employee_id: Uuid,
    pub employee_name: Option<String>,
    pub time_entry_id: Option<Uuid>,
    pub roster_id: Option<Uuid>,
    pub flag_type: String,
    pub severity: String,
    pub message: String,
    pub flag_date: Option<NaiveDate>,
    pub is_resolved: bool,
    pub resolved_by: Option<Uuid>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_note: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub organization_id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct ResolveFlagRequest {
    #[serde(default)]
    pub resolution_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FlagsQuery {
    pub organization_id: String,
    #[serde(default)]
    pub resolved: Option<bool>,
    #[serde(default)]
    pub employee_id: Option<Uuid>,
    #[serde(default)]
    pub flag_type: Option<String>,
    #[serde(default)]
    pub start: Option<NaiveDate>,
    #[serde(default)]
    pub end: Option<NaiveDate>,
}

// ── Leave Balances ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveBalance {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub employee_id: Uuid,
    pub employee_name: Option<String>,
    pub leave_type: String,
    pub balance_hours: f64,
    pub accrual_rate: f64,
    pub max_balance: Option<f64>,
    pub year: i32,
}

#[derive(Debug, Deserialize)]
pub struct AdjustLeaveBalanceRequest {
    pub organization_id: String,
    pub employee_id: Uuid,
    pub leave_type: String,
    pub balance_hours: f64,
    #[serde(default)]
    pub accrual_rate: Option<f64>,
    #[serde(default)]
    pub max_balance: Option<f64>,
    #[serde(default)]
    pub year: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct LeaveBalanceQuery {
    pub organization_id: String,
    #[serde(default)]
    pub employee_id: Option<Uuid>,
    #[serde(default)]
    pub year: Option<i32>,
}

// ── Reports ──

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub organization_id: String,
    #[serde(default)]
    pub employee_id: Option<Uuid>,
    #[serde(default)]
    pub start: Option<NaiveDate>,
    #[serde(default)]
    pub end: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct FlsaReportQuery {
    pub organization_id: String,
    pub cycle_start: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct PayrollExportQuery {
    pub organization_id: String,
    pub period_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct HoursReport {
    pub employee_id: Uuid,
    pub employee_name: String,
    pub rank: String,
    pub total_hours: f64,
    pub regular_hours: f64,
    pub overtime_hours: f64,
    pub leave_hours: f64,
    pub by_pay_code: Vec<PayCodeHours>,
}

#[derive(Debug, Serialize)]
pub struct PayCodeHours {
    pub pay_code: String,
    pub display_name: String,
    pub hours: f64,
    pub category: String,
}

#[derive(Debug, Serialize)]
pub struct FlsaReport {
    pub cycle_start: NaiveDate,
    pub cycle_end: NaiveDate,
    pub threshold_hours: f64,
    pub employees: Vec<FlsaEmployeeReport>,
}

#[derive(Debug, Serialize)]
pub struct FlsaEmployeeReport {
    pub employee_id: Uuid,
    pub employee_name: String,
    pub flsa_hours: f64,
    pub threshold: f64,
    pub overtime_hours: f64,
    pub is_compliant: bool,
}

#[derive(Debug, Serialize)]
pub struct StaffingReport {
    pub date: NaiveDate,
    pub shift_label: String,
    pub stations: Vec<StationStaffing>,
    pub total_on_duty: i32,
    pub total_absent: i32,
    pub alerts: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct StationStaffing {
    pub station_id: Uuid,
    pub station_name: String,
    pub min_staffing: i32,
    pub actual_staffing: i32,
    pub is_below_minimum: bool,
    pub personnel: Vec<String>,
}
