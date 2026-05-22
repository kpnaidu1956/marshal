use crate::audit::logger::AuditLogger;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

use std::sync::OnceLock;

/// AES-256-GCM encryption key, loaded once from BPE_CREDENTIAL_KEY env var (base64-encoded, 32 bytes).
/// Falls back to a static key ONLY if the env var is not set (logs a warning).
static CREDENTIAL_KEY: OnceLock<[u8; 32]> = OnceLock::new();

fn get_credential_key() -> &'static [u8; 32] {
    CREDENTIAL_KEY.get_or_init(|| {
        if let Ok(b64) = std::env::var("BPE_CREDENTIAL_KEY") {
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                b64.trim(),
            ).expect("BPE_CREDENTIAL_KEY must be valid base64");
            let mut key = [0u8; 32];
            assert!(decoded.len() == 32, "BPE_CREDENTIAL_KEY must be exactly 32 bytes");
            key.copy_from_slice(&decoded);
            key
        } else {
            tracing::warn!("BPE_CREDENTIAL_KEY not set — using static fallback key. Set this env var in production!");
            // Deterministic fallback — NOT safe for production
            let mut key = [0u8; 32];
            let src = b"bpe-integration-key-2026-marshal";
            key.copy_from_slice(src);
            key
        }
    })
}

pub struct IntegrationEngine;

impl IntegrationEngine {
    // ---- Credential CRUD ----

    pub async fn create_credential(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        req: &CreateCredentialRequest,
    ) -> Result<IntegrationCredential, BpeError> {
        // Validate integration type
        if !INTEGRATION_TYPES.iter().any(|t| t.name == req.integration_type) {
            return Err(BpeError::BadRequest(format!(
                "Unknown integration type: '{}'. Valid types: {}",
                req.integration_type,
                INTEGRATION_TYPES.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
            )));
        }

        let cred_bytes = serde_json::to_vec(&req.credentials)
            .map_err(|e| BpeError::Internal(format!("JSON serialization error: {e}")))?;
        let (encrypted, nonce) = obfuscate(&cred_bytes);

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.integration_credentials
                    (organization_id, integration_type, name, encrypted_credentials,
                     encryption_nonce, created_by)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id, organization_id, integration_type, name,
                           last_test_at, last_test_success, last_test_error,
                           is_active, created_at, updated_at, created_by",
                &[&org_id, &req.integration_type, &req.name, &encrypted, &nonce, &user_id],
            )
            .await?;

        let cred = row_to_credential(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "integration_credential.created", "integration_credential", cred.id,
            Some(user_id), None, None,
            serde_json::json!({ "integration_type": req.integration_type, "name": req.name }),
        ).await {
            tracing::warn!("Audit log failed for integration_credential.created: {e}");
        }

        Ok(cred)
    }

    pub async fn list_credentials(
        pool: &PgPool,
        org_id: Uuid,
        integration_type: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<crate::entity::models::PaginatedResponse<CredentialSummary>, BpeError> {
        let client = pool.get().await?;
        let offset = (page - 1) * per_page;

        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.integration_credentials
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR integration_type = $2)",
                &[&org_id, &integration_type],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, integration_type, name, is_active,
                        last_test_at, last_test_success, last_test_error, created_at
                 FROM bpe.integration_credentials
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR integration_type = $2)
                 ORDER BY integration_type, name
                 LIMIT $3 OFFSET $4",
                &[&org_id, &integration_type, &per_page, &offset],
            )
            .await?;

        let data = rows.iter().map(|row| CredentialSummary {
            id: row.get("id"),
            integration_type: row.get("integration_type"),
            name: row.get("name"),
            is_active: row.get("is_active"),
            last_test_at: row.get("last_test_at"),
            last_test_success: row.get("last_test_success"),
            last_test_error: row.get("last_test_error"),
            created_at: row.get("created_at"),
        }).collect();
        Ok(crate::entity::models::PaginatedResponse { data, page, per_page, total })
    }

    pub async fn get_credential(pool: &PgPool, id: Uuid) -> Result<IntegrationCredential, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, integration_type, name,
                        last_test_at, last_test_success, last_test_error,
                        is_active, created_at, updated_at, created_by
                 FROM bpe.integration_credentials WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Integration credential {id} not found")))?;

        Ok(row_to_credential(&row))
    }

    pub async fn update_credential(
        pool: &PgPool,
        id: Uuid,
        req: &UpdateCredentialRequest,
    ) -> Result<IntegrationCredential, BpeError> {
        // Use a single connection for both the fetch and the update
        let client = pool.get().await?;
        let existing = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, integration_type, name,
                            last_test_at, last_test_success, last_test_error,
                            is_active, created_at, updated_at, created_by
                     FROM bpe.integration_credentials WHERE id = $1",
                    &[&id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Integration credential {id} not found")))?;
            row_to_credential(&row)
        };

        let name = req.name.as_deref().unwrap_or(&existing.name);
        let is_active = req.is_active.unwrap_or(existing.is_active);

        // If credentials are being updated, re-encrypt
        if let Some(new_creds) = &req.credentials {
            let cred_bytes = serde_json::to_vec(new_creds)
                .map_err(|e| BpeError::Internal(format!("JSON serialization error: {e}")))?;
            let (encrypted, nonce) = obfuscate(&cred_bytes);

            let row = client
                .query_one(
                    "UPDATE bpe.integration_credentials
                     SET name=$1, encrypted_credentials=$2, encryption_nonce=$3,
                         is_active=$4, updated_at=now()
                     WHERE id=$5
                     RETURNING id, organization_id, integration_type, name,
                               last_test_at, last_test_success, last_test_error,
                               is_active, created_at, updated_at, created_by",
                    &[&name, &encrypted, &nonce, &is_active, &id],
                )
                .await?;
            Ok(row_to_credential(&row))
        } else {
            let row = client
                .query_one(
                    "UPDATE bpe.integration_credentials
                     SET name=$1, is_active=$2, updated_at=now()
                     WHERE id=$3
                     RETURNING id, organization_id, integration_type, name,
                               last_test_at, last_test_success, last_test_error,
                               is_active, created_at, updated_at, created_by",
                    &[&name, &is_active, &id],
                )
                .await?;
            Ok(row_to_credential(&row))
        }
    }

    pub async fn delete_credential(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute("DELETE FROM bpe.integration_credentials WHERE id = $1", &[&id])
            .await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Integration credential {id} not found")));
        }
        Ok(())
    }

    /// Test a credential by attempting to validate it.
    pub async fn test_credential(pool: &PgPool, id: Uuid) -> Result<IntegrationCredential, BpeError> {
        let cred = Self::get_credential(pool, id).await?;

        // Retrieve the encrypted credentials
        let client = pool.get().await?;
        let row = client
            .query_one(
                "SELECT encrypted_credentials, encryption_nonce FROM bpe.integration_credentials WHERE id = $1",
                &[&id],
            )
            .await?;

        let encrypted: Vec<u8> = row.get("encrypted_credentials");
        let nonce: Vec<u8> = row.get("encryption_nonce");
        let decrypted = deobfuscate(&encrypted, &nonce);

        let creds: serde_json::Value = serde_json::from_slice(&decrypted)
            .map_err(|e| BpeError::Internal(format!("Failed to decode credentials: {e}")))?;

        // Perform a basic validation based on integration type
        let (success, error) = validate_credentials(&cred.integration_type, &creds);

        // Update test results
        let updated_row = client
            .query_one(
                "UPDATE bpe.integration_credentials
                 SET last_test_at=now(), last_test_success=$2, last_test_error=$3, updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, integration_type, name,
                           last_test_at, last_test_success, last_test_error,
                           is_active, created_at, updated_at, created_by",
                &[&id, &success, &error],
            )
            .await?;

        Ok(row_to_credential(&updated_row))
    }

    /// Execute an integration action.
    /// For `ruflo_agent` type, delegates to the Ruflo sidecar service.
    /// For other types, returns a simulated result (real HTTP calls TBD).
    pub async fn execute(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Option<Uuid>,
        req: &ExecuteIntegrationRequest,
        ruflo_base_url: &str,
    ) -> Result<IntegrationResult, BpeError> {
        let start = std::time::Instant::now();

        // Validate integration type and action
        let int_type = INTEGRATION_TYPES.iter()
            .find(|t| t.name == req.integration_type)
            .ok_or_else(|| BpeError::BadRequest(format!("Unknown integration type: '{}'", req.integration_type)))?;

        if !int_type.actions.contains(&req.action.as_str()) {
            return Err(BpeError::BadRequest(format!(
                "Invalid action '{}' for integration type '{}'. Valid: {}",
                req.action, req.integration_type, int_type.actions.join(", ")
            )));
        }

        // If a credential_id is provided, verify it exists and is active
        if let Some(cred_id) = req.credential_id {
            let cred = Self::get_credential(pool, cred_id).await?;
            if !cred.is_active {
                return Err(BpeError::BadRequest("Integration credential is not active".into()));
            }
            if cred.organization_id != org_id {
                return Err(BpeError::Forbidden("Credential belongs to a different organization".into()));
            }
        }

        let params = req.parameters.clone().unwrap_or(serde_json::json!({}));

        // Route to Ruflo agent execution
        let result = if req.integration_type == "ruflo_agent" {
            Self::execute_ruflo_agent(&req.action, &params, ruflo_base_url).await?
        } else {
            // Simulated result for other integration types
            let duration_ms = start.elapsed().as_millis() as i64;
            IntegrationResult {
                success: true,
                output: serde_json::json!({
                    "integration_type": req.integration_type,
                    "action": req.action,
                    "parameters": params,
                    "message": format!("Integration '{}:{}' executed successfully (simulated)", req.integration_type, req.action),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
                error: None,
                duration_ms,
            }
        };

        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "integration.executed", "integration", Uuid::nil(),
            user_id, None, None,
            serde_json::json!({
                "integration_type": req.integration_type,
                "action": req.action,
                "success": result.success,
                "duration_ms": result.duration_ms,
            }),
        ).await {
            tracing::warn!("Audit log failed for integration.executed: {e}");
        }

        Ok(result)
    }

    /// Execute a Ruflo AI agent action.
    ///
    /// If `agent_type` is not explicitly provided in params, the system automatically
    /// infers the best agent from the step name, description, prompt, workflow category,
    /// and preceding step types.
    async fn execute_ruflo_agent(
        _action: &str,
        params: &serde_json::Value,
        ruflo_base_url: &str,
    ) -> Result<IntegrationResult, BpeError> {
        let client = super::ruflo::RufloClient::new(ruflo_base_url);

        let prompt = params.get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Auto-infer agent type from step context if not explicitly set
        let agent_type = if let Some(explicit) = params.get("agent_type").and_then(|v| v.as_str()) {
            explicit.to_string()
        } else {
            let step_name = params.get("step_name").and_then(|v| v.as_str()).unwrap_or("");
            let step_desc = params.get("step_description").and_then(|v| v.as_str()).unwrap_or("");
            let category = params.get("workflow_category").and_then(|v| v.as_str()).unwrap_or("general");
            let preceding: Vec<&str> = params.get("preceding_step_types")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect())
                .unwrap_or_default();

            let inferred = super::ruflo::infer_agent_type(step_name, step_desc, &prompt, category, &preceding);
            tracing::info!("Auto-selected Ruflo agent type '{}' for step '{}'", inferred, step_name);
            inferred.to_string()
        };

        let tools: Vec<String> = params.get("tools")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let context = params.get("context")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let req = RufloAgentRequest {
            agent_type: agent_type.clone(),
            prompt,
            tools,
            context,
            callback_url: None,
        };

        let mut result = super::ruflo::ruflo_response_to_result(&client.spawn_agent(&req).await?);
        // Include the inferred agent type in the output for transparency
        if let Some(obj) = result.output.as_object_mut() {
            obj.insert("agent_type_used".to_string(), serde_json::json!(agent_type));
        }
        Ok(result)
    }

    /// List available integration types and their actions.
    pub fn list_types() -> &'static [IntegrationType] {
        INTEGRATION_TYPES
    }
}

// ---- Encryption helpers (AES-256-GCM) ----

fn obfuscate(data: &[u8]) -> (Vec<u8>, Vec<u8>) {
    use ring::aead;

    let key_bytes = get_credential_key();
    let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .expect("Invalid AES-256-GCM key");
    let sealing_key = aead::LessSafeKey::new(unbound_key);

    // Generate a random 12-byte nonce (standard for AES-GCM)
    let rng = ring::rand::SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    ring::rand::SecureRandom::fill(&rng, &mut nonce_bytes)
        .expect("RNG failed");

    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = data.to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut in_out)
        .expect("AES-GCM encryption failed");

    (in_out, nonce_bytes.to_vec())
}

fn deobfuscate(encrypted: &[u8], nonce: &[u8]) -> Vec<u8> {
    use ring::aead;

    let key_bytes = get_credential_key();
    let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .expect("Invalid AES-256-GCM key");
    let opening_key = aead::LessSafeKey::new(unbound_key);

    let mut nonce_arr = [0u8; 12];
    // Handle legacy 16-byte nonces by truncating (backward compat for existing data)
    let n = nonce.len().min(12);
    nonce_arr[..n].copy_from_slice(&nonce[..n]);
    let nonce = aead::Nonce::assume_unique_for_key(nonce_arr);

    let mut in_out = encrypted.to_vec();
    match opening_key.open_in_place(nonce, aead::Aad::empty(), &mut in_out) {
        Ok(plaintext) => plaintext.to_vec(),
        Err(_) => {
            // Fallback: try legacy XOR decryption for data encrypted before this change
            tracing::warn!("AES-GCM decryption failed, trying legacy XOR decryption");
            legacy_deobfuscate(encrypted, &nonce_arr)
        }
    }
}

/// Legacy XOR decryption for backward compatibility with pre-AES data.
fn legacy_deobfuscate(encrypted: &[u8], nonce: &[u8]) -> Vec<u8> {
    let legacy_key = b"bpe-integration-key-2026-marshal";
    let mut data = encrypted.to_vec();
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= legacy_key[i % legacy_key.len()] ^ nonce[i % nonce.len()];
    }
    data
}

/// Validate credential fields for a given integration type.
fn validate_credentials(integration_type: &str, creds: &serde_json::Value) -> (bool, Option<String>) {
    let int_type = match INTEGRATION_TYPES.iter().find(|t| t.name == integration_type) {
        Some(t) => t,
        None => return (false, Some(format!("Unknown integration type: {integration_type}"))),
    };

    // Check that required credential fields are present
    let obj = match creds.as_object() {
        Some(o) => o,
        None => return (false, Some("Credentials must be a JSON object".into())),
    };

    let missing: Vec<&str> = int_type.credential_fields.iter()
        .filter(|f| !obj.contains_key(**f) || obj[**f].as_str().map_or(true, |s| s.is_empty()))
        .copied()
        .collect();

    if missing.is_empty() {
        (true, None)
    } else {
        (false, Some(format!("Missing or empty fields: {}", missing.join(", "))))
    }
}

// ---- Row converter ----

fn row_to_credential(row: &tokio_postgres::Row) -> IntegrationCredential {
    IntegrationCredential {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        integration_type: row.get("integration_type"),
        name: row.get("name"),
        last_test_at: row.get("last_test_at"),
        last_test_success: row.get("last_test_success"),
        last_test_error: row.get("last_test_error"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        created_by: row.get("created_by"),
    }
}
