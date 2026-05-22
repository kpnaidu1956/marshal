pub mod acl;
pub mod approvals;
pub mod audit;
pub mod entities;
pub mod entity_types;
pub mod health;
pub mod integrations;
pub mod knowledge;
pub mod reports;
pub mod ruflo;
pub mod timekeeping;
pub mod workflows;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use crate::AppState;

/// Build the protected API route tree.
pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::authenticated_health))
        .route("/metrics", get(health::metrics_endpoint))
        // Entity types
        .route("/entity-types", get(entity_types::list))
        .route("/entity-types", post(entity_types::create))
        .route("/entity-types/:id", put(entity_types::update))
        .route("/entity-types/:id", delete(entity_types::delete))
        // Entities
        .route("/entities", get(entities::list))
        .route("/entities", post(entities::create))
        .route("/entities/:id", get(entities::get))
        .route("/entities/:id", put(entities::update))
        .route("/entities/:id", delete(entities::delete))
        // Relationships
        .route("/entities/:id/relationships", get(entities::list_relationships))
        .route("/entities/:id/relationships", post(entities::add_relationship))
        .route("/entity-relationships/:id", delete(entities::remove_relationship))
        // Interactions
        .route("/entities/:id/interactions", get(entities::list_interactions))
        .route("/entities/:id/interactions", post(entities::add_interaction))
        // Audit trail
        .route("/audit/events", get(audit::list_events))
        .route("/audit/events", post(audit::create_event))
        .route("/audit/entity/:entity_id", get(audit::entity_events))
        .route("/audit/resource/:resource_type/:resource_id", get(audit::resource_events))
        .route("/audit/reversal", post(audit::reverse_event))
    // Workflow definitions
    .route("/workflows/definitions", get(workflows::list_definitions))
    .route("/workflows/definitions", post(workflows::create_definition))
    .route("/workflows/definitions/:id", put(workflows::update_definition))
    .route("/workflows/definitions/:id", delete(workflows::delete_definition))
    .route("/workflows/definitions/:id/execute", post(workflows::execute_definition))
    // Workflow executions
    .route("/workflows/executions", get(workflows::list_executions))
    .route("/workflows/executions/:id", get(workflows::get_execution))
    .route("/workflows/executions/:id/confirm", post(workflows::confirm))
    .route("/workflows/executions/:id/start", post(workflows::start_execution))
    .route("/workflows/executions/:id/pause", post(workflows::pause_execution))
    .route("/workflows/executions/:id/resume", post(workflows::resume_execution))
    .route("/workflows/executions/:id/cancel", post(workflows::cancel_execution))
    .route("/workflows/executions/:id/timeline", get(workflows::timeline))
    // Workflow steps
    .route("/workflows/steps/:id/complete", post(workflows::complete_step))
    .route("/workflows/steps/:id/skip", post(workflows::skip_step))
    .route("/workflows/steps/:id/retry", post(workflows::retry_step))
    .route("/workflows/steps/:id/assign", post(workflows::assign_step))
    // Approval rules
    .route("/approvals/rules", get(approvals::list_rules))
    .route("/approvals/rules", post(approvals::create_rule))
    .route("/approvals/rules/:id", get(approvals::get_rule))
    .route("/approvals/rules/:id", put(approvals::update_rule))
    .route("/approvals/rules/:id", delete(approvals::delete_rule))
    // Approval requests
    .route("/approvals/requests", get(approvals::list_requests))
    .route("/approvals/requests", post(approvals::create_request))
    .route("/approvals/requests/:id", get(approvals::get_request))
    .route("/approvals/requests/:id/cancel", post(approvals::cancel_request))
    .route("/approvals/requests/:id/decide", post(approvals::decide))
    .route("/approvals/requests/:id/decisions", get(approvals::list_decisions))
    .route("/approvals/pending", get(approvals::pending_for_me))
    // Integration types & credentials
    .route("/integrations/types", get(integrations::list_types))
    .route("/integrations/credentials", get(integrations::list_credentials))
    .route("/integrations/credentials", post(integrations::create_credential))
    .route("/integrations/credentials/:id", get(integrations::get_credential))
    .route("/integrations/credentials/:id", put(integrations::update_credential))
    .route("/integrations/credentials/:id", delete(integrations::delete_credential))
    .route("/integrations/credentials/:id/test", post(integrations::test_credential))
    .route("/integrations/execute", post(integrations::execute))
    // Ruflo AI agent integration
    .route("/ruflo/health", get(ruflo::ruflo_health))
    .route("/ruflo/agent-types", get(ruflo::list_agent_types))
    .route("/ruflo/agent/spawn", post(ruflo::spawn_agent))
    .route("/ruflo/callback/:step_id", post(ruflo::agent_callback))
    // Knowledge learning
    .route("/knowledge/learn", post(knowledge::learn_from_execution))
    .route("/knowledge/learn-from-goal", post(knowledge::learn_from_goal))
    .route("/knowledge/suggest", post(knowledge::suggest))
    .route("/knowledge/sequences", get(knowledge::list_sequences))
    .route("/knowledge/sequences/:id", get(knowledge::get_sequence))
    .route("/knowledge/sequences/:id", delete(knowledge::deactivate_sequence))
    .route("/knowledge/sequences/:id/feedback", post(knowledge::feedback))
    .route("/knowledge/sequences/:id/promote", post(knowledge::promote_to_definition))
    // Report templates
    .route("/reports/templates", get(reports::list_templates))
    .route("/reports/templates", post(reports::create_template))
    .route("/reports/templates/:id", get(reports::get_template))
    .route("/reports/templates/:id", put(reports::update_template))
    .route("/reports/templates/:id", delete(reports::delete_template))
    .route("/reports/templates/:id/run", post(reports::run_report))
    // Built-in reports
    .route("/reports/dashboard", get(reports::dashboard))
    .route("/reports/workflow-performance", get(reports::workflow_performance))
    // Notifications
    .route("/notifications", get(reports::list_notifications))
    .route("/notifications", post(reports::create_notification))
    .route("/notifications/unread-count", get(reports::unread_count))
    .route("/notifications/mark-read", post(reports::mark_read))
    .route("/notifications/mark-all-read", post(reports::mark_all_read))
    // Document ACLs
    .route("/documents/:id/acls", get(acl::list_document_acls))
    .route("/documents/:id/acls", post(acl::create_document_acl))
    .route("/documents/:id/acls/clear", delete(acl::clear_document_acls))
    .route("/documents/:id/acls/:acl_id", delete(acl::delete_document_acl))
    // Groups
    .route("/groups", get(acl::list_groups))
    .route("/groups", post(acl::create_group))
    .route("/groups/:id", put(acl::update_group))
    .route("/groups/:id", delete(acl::delete_group))
    .route("/groups/:id/members", get(acl::list_group_members))
    .route("/groups/:id/members", post(acl::add_group_member))
    .route("/groups/:id/members/:user_id", delete(acl::remove_group_member))
    .route("/groups/:id/permissions", get(acl::list_group_permissions))
    .route("/groups/:id/permissions", post(acl::add_group_permission))
    .route("/groups/:id/permissions/:perm_id", delete(acl::remove_group_permission))
    // Permission introspection
    .route("/permissions/me", get(acl::my_permissions))
    .route("/admin/cache/invalidate", post(acl::invalidate_cache))
    // Timekeeping module
    .nest("/timekeeping", timekeeping::timekeeping_routes())
}
