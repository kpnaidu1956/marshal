mod common;

use axum::body::Body;
use axum::http::Request;
use common::*;

// ──────────────────────────── Health ────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let app = require_db!(test_app().await);
    let req = Request::builder()
        .uri("/bpe/health")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "bpe-server");
    assert!(body["db_connected"].as_bool().unwrap());
}

// ──────────────────────── Authentication ────────────────────────

#[tokio::test]
async fn protected_endpoint_requires_auth() {
    let app = require_db!(test_app().await);
    // No auth header → 401
    let req = Request::builder()
        .uri("/bpe/api/health")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"], "Missing authorization token");
}

#[tokio::test]
async fn invalid_token_returns_401() {
    let app = require_db!(test_app().await);
    let req = Request::builder()
        .uri("/bpe/api/health")
        .header("authorization", "Bearer invalid.token.here")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"], "Invalid token");
}

#[tokio::test]
async fn wrong_secret_returns_401() {
    let app = require_db!(test_app().await);
    let token = make_jwt(TEST_USER_ID, TEST_EMAIL, TEST_ORG_UUID, false, "wrong-secret");
    let req = get_req("/bpe/api/health", &token);
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"], "Invalid token");
}

#[tokio::test]
async fn expired_token_returns_401() {
    let app = require_db!(test_app().await);
    let token = make_expired_jwt(TEST_JWT_SECRET);
    let req = get_req("/bpe/api/health", &token);
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"], "Token expired");
}

#[tokio::test]
async fn valid_token_passes_auth() {
    let app = require_db!(test_app().await);
    let req = get_req("/bpe/api/health", &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
}

// ──────────────────────── Metrics ────────────────────────

#[tokio::test]
async fn metrics_requires_auth() {
    let app = require_db!(test_app().await);
    let req = Request::builder()
        .uri("/bpe/api/metrics")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn metrics_returns_data() {
    let app = require_db!(test_app().await);
    let req = get_req("/bpe/api/metrics", &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["uptime_seconds"].is_number());
    assert!(body["total_requests"].is_number());
}

// ──────────────────────── Entity Types ────────────────────────

#[tokio::test]
async fn list_entity_types() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/entity-types?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn create_entity_type_validates_name() {
    let app = require_db!(test_app().await);
    // Empty name → 400
    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": "",
        "schema": {}
    });
    let req = post_json("/bpe/api/entity-types", &test_token(), &body);
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 400);
}

// ──────────────────────── Entities ────────────────────────

#[tokio::test]
async fn list_entities() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/entities?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn get_entity_wrong_org_returns_403() {
    let app = require_db!(test_app().await);
    // Use a token with a different org than the resource
    let other_org = "00000000-0000-0000-0000-000000000099";
    let token = make_jwt(TEST_USER_ID, TEST_EMAIL, other_org, false, TEST_JWT_SECRET);

    // First list entities from the real org to get an ID
    let uri = format!("/bpe/api/entities?organization_id={TEST_ORG_SLUG}");
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    if status != 200 {
        eprintln!("SKIPPED: could not list entities");
        return;
    }

    let entities = body["data"].as_array().unwrap();
    if entities.is_empty() {
        eprintln!("SKIPPED: no entities to test org access against");
        return;
    }

    let entity_id = entities[0]["id"].as_str().unwrap();
    let req = get_req(&format!("/bpe/api/entities/{entity_id}"), &token);
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 403);
    assert!(body["error"].as_str().unwrap().contains("Access denied"));
}

// ──────────────────────── Workflow Definitions ────────────────────────

#[tokio::test]
async fn list_workflow_definitions_paginated() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/workflows/definitions?organization_id={}&page=1&per_page=5",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
    assert!(body["page"].is_number());
    assert!(body["per_page"].is_number());
    assert!(body["total"].is_number());
}

#[tokio::test]
async fn create_workflow_definition_validates() {
    let app = require_db!(test_app().await);

    // Missing name → 400
    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": "",
        "category": "general",
        "step_templates": []
    });
    let req = post_json("/bpe/api/workflows/definitions", &test_token(), &body);
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 400);

    // Valid creation
    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": format!("Test Workflow {}", uuid::Uuid::new_v4()),
        "category": "general",
        "step_templates": [{"name": "Step 1", "step_type": "manual"}]
    });
    let req = post_json("/bpe/api/workflows/definitions", &test_token(), &body);
    let (status, resp) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(resp["data"]["id"].is_string());

    // Clean up — delete the created definition
    let def_id = resp["data"]["id"].as_str().unwrap();
    let req = delete_req(
        &format!("/bpe/api/workflows/definitions/{def_id}"),
        &test_token(),
    );
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn update_workflow_definition_validates() {
    let app = require_db!(test_app().await);

    // Create a definition first
    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": format!("Update Test {}", uuid::Uuid::new_v4()),
        "category": "general",
        "step_templates": [{"name": "Step 1", "step_type": "manual"}]
    });
    let req = post_json("/bpe/api/workflows/definitions", &test_token(), &body);
    let (status, resp) = send(&app, req).await;
    if status != 200 {
        eprintln!("SKIPPED: could not create test definition");
        return;
    }
    let def_id = resp["data"]["id"].as_str().unwrap();

    // Update with empty name → 400
    let body = serde_json::json!({ "name": "   " });
    let req = put_json(
        &format!("/bpe/api/workflows/definitions/{def_id}"),
        &test_token(),
        &body,
    );
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 400);

    // Update with valid name
    let body = serde_json::json!({ "name": "Renamed Workflow" });
    let req = put_json(
        &format!("/bpe/api/workflows/definitions/{def_id}"),
        &test_token(),
        &body,
    );
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 200);

    // Clean up
    let req = delete_req(
        &format!("/bpe/api/workflows/definitions/{def_id}"),
        &test_token(),
    );
    send(&app, req).await;
}

// ──────────────────────── Approval Rules ────────────────────────

#[tokio::test]
async fn list_approval_rules_paginated() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/approvals/rules?organization_id={}&page=1&per_page=10",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
    assert!(body["total"].is_number());
}

#[tokio::test]
async fn create_approval_rule_validates() {
    let app = require_db!(test_app().await);

    // Empty name → 400
    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": "",
        "entity_type": "any",
        "conditions": {},
        "approver_user_ids": [TEST_USER_ID]
    });
    let req = post_json("/bpe/api/approvals/rules", &test_token(), &body);
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 400);
}

// ──────────────────────── Integration Credentials ────────────────────────

#[tokio::test]
async fn list_credentials_paginated() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/integrations/credentials?organization_id={}&page=1&per_page=10",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
    assert!(body["total"].is_number());
}

#[tokio::test]
async fn list_integration_types() {
    let app = require_db!(test_app().await);
    let req = get_req("/bpe/api/integrations/types", &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
    assert!(!body["data"].as_array().unwrap().is_empty());
}

// ──────────────────────── Report Templates ────────────────────────

#[tokio::test]
async fn list_report_templates_paginated() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/reports/templates?organization_id={}&page=1&per_page=10",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
    assert!(body["total"].is_number());
}

#[tokio::test]
async fn create_and_delete_report_template() {
    let app = require_db!(test_app().await);

    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": format!("Test Report {}", uuid::Uuid::new_v4()),
        "category": "general",
        "sql_template": "SELECT COUNT(*) as total FROM bpe.entities WHERE organization_id = $org_id"
    });
    let req = post_json("/bpe/api/reports/templates", &test_token(), &body);
    let (status, resp) = send(&app, req).await;
    assert_eq!(status, 200);
    let tpl_id = resp["data"]["id"].as_str().unwrap();

    // Run the report
    let run_body = serde_json::json!({ "organization_id": TEST_ORG_SLUG });
    let req = post_json(
        &format!("/bpe/api/reports/templates/{tpl_id}/run"),
        &test_token(),
        &run_body,
    );
    let (status, resp) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(resp["data"]["row_count"].is_number());

    // Delete
    let req = delete_req(
        &format!("/bpe/api/reports/templates/{tpl_id}"),
        &test_token(),
    );
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn create_report_template_validates_sql() {
    let app = require_db!(test_app().await);

    // Empty SQL → 400
    let body = serde_json::json!({
        "organization_id": TEST_ORG_SLUG,
        "name": "Bad Template",
        "category": "general",
        "sql_template": ""
    });
    let req = post_json("/bpe/api/reports/templates", &test_token(), &body);
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 400);
}

// ──────────────────────── Dashboard / Built-in Reports ────────────────────────

#[tokio::test]
async fn dashboard_returns_data() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/reports/dashboard?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_object());
}

#[tokio::test]
async fn workflow_performance_returns_data() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/reports/workflow-performance?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

// ──────────────────────── Knowledge / Sequences ────────────────────────

#[tokio::test]
async fn list_learned_sequences() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/knowledge/sequences?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

// ──────────────────────── Notifications ────────────────────────

#[tokio::test]
async fn list_notifications() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/notifications?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn unread_notification_count() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/notifications/unread-count?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["unread_count"].is_number());
}

// ──────────────────────── Audit Events ────────────────────────

#[tokio::test]
async fn list_audit_events() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/audit/events?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

// ──────────────────────── 404 for unknown routes ────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = require_db!(test_app().await);
    let req = get_req("/bpe/api/nonexistent", &test_token());
    let (status, _) = send(&app, req).await;
    assert_eq!(status, 404);
}

// ──────────────────────── Workflow Executions ────────────────────────

#[tokio::test]
async fn list_workflow_executions() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/workflows/executions?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

// ──────────────────────── Approval Requests ────────────────────────

#[tokio::test]
async fn list_approval_requests() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/approvals/requests?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn pending_approvals_for_me() {
    let app = require_db!(test_app().await);
    let uri = format!(
        "/bpe/api/approvals/pending?organization_id={}",
        TEST_ORG_SLUG
    );
    let req = get_req(&uri, &test_token());
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert!(body["data"].is_array());
}
