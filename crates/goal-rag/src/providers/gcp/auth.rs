//! GCP authentication using service account
//!
//! Handles OAuth2 token generation for Vertex AI and GCS APIs.

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{Error, Result};

/// GCP authentication manager
pub struct GcpAuth {
    /// Service account key path
    key_path: String,
    /// Project ID
    project_id: String,
    /// Cached access token
    token: Arc<RwLock<Option<CachedToken>>>,
}

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at: std::time::Instant,
}

impl GcpAuth {
    /// Create from service account JSON key file
    pub fn from_service_account(key_path: impl AsRef<Path>, project_id: String) -> Result<Self> {
        let key_path = key_path.as_ref().to_string_lossy().to_string();
        if !Path::new(&key_path).exists() {
            return Err(Error::Config(format!(
                "Service account key not found: {}",
                key_path
            )));
        }

        Ok(Self {
            key_path,
            project_id,
            token: Arc::new(RwLock::new(None)),
        })
    }

    /// Get project ID
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Get a valid access token (refreshing if needed)
    pub async fn get_token(&self) -> Result<String> {
        // Check if cached token is still valid
        {
            let token = self.token.read().await;
            if let Some(ref cached) = *token {
                // Token valid for at least 60 more seconds
                if cached.expires_at > std::time::Instant::now() + std::time::Duration::from_secs(60)
                {
                    return Ok(cached.access_token.clone());
                }
            }
        }

        // Need to refresh token
        let new_token = self.refresh_token().await?;

        // Cache it
        {
            let mut token = self.token.write().await;
            *token = Some(CachedToken {
                access_token: new_token.clone(),
                // Tokens typically valid for 1 hour, assume 55 minutes to be safe
                expires_at: std::time::Instant::now() + std::time::Duration::from_secs(55 * 60),
            });
        }

        Ok(new_token)
    }

    /// Refresh the access token from service account using JWT
    async fn refresh_token(&self) -> Result<String> {
        // Read the service account key file
        let key_content = tokio::fs::read_to_string(&self.key_path).await.map_err(|e| {
            Error::Config(format!(
                "Failed to read service account key {}: {}",
                self.key_path, e
            ))
        })?;

        #[derive(serde::Deserialize)]
        struct ServiceAccountKey {
            client_email: String,
            private_key: String,
            token_uri: String,
        }

        let key: ServiceAccountKey = serde_json::from_str(&key_content).map_err(|e| {
            Error::Config(format!("Invalid service account key format: {}", e))
        })?;

        // Create JWT for service account authentication
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let claims = serde_json::json!({
            "iss": key.client_email,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "aud": key.token_uri,
            "iat": now,
            "exp": now + 3600,
        });

        // Sign the JWT using RS256
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"RS256","typ":"JWT"}"#.as_bytes());
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(claims.to_string().as_bytes());

        let signing_input = format!("{}.{}", header, payload);

        // Parse the private key and sign
        let private_key = key.private_key.replace("\\n", "\n");
        let key_pair = ring::signature::RsaKeyPair::from_pkcs8(
            pem::parse(&private_key)
                .map_err(|e| Error::Config(format!("Failed to parse private key PEM: {}", e)))?
                .contents(),
        )
        .map_err(|e| Error::Config(format!("Failed to parse private key: {:?}", e)))?;

        let mut signature = vec![0u8; key_pair.public().modulus_len()];
        key_pair
            .sign(
                &ring::signature::RSA_PKCS1_SHA256,
                &ring::rand::SystemRandom::new(),
                signing_input.as_bytes(),
                &mut signature,
            )
            .map_err(|e| Error::Config(format!("Failed to sign JWT: {:?}", e)))?;

        let signature_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&signature);
        let jwt = format!("{}.{}", signing_input, signature_b64);

        // Exchange JWT for access token
        let client = reqwest::Client::new();
        let response = client
            .post(&key.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .send()
            .await
            .map_err(|e| Error::Config(format!("Token exchange request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Config(format!(
                "Token exchange failed ({}): {}",
                status, body
            )));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
        }

        let token_response: TokenResponse = response.json().await.map_err(|e| {
            Error::Config(format!("Failed to parse token response: {}", e))
        })?;

        Ok(token_response.access_token)
    }

    /// Create HTTP client with auth headers
    pub async fn authorized_client(&self) -> Result<reqwest::Client> {
        let token = self.get_token().await?;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", token).parse().unwrap(),
        );

        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| Error::Internal(format!("Failed to build HTTP client: {}", e)))
    }
}
