//! Resend email client for transactional emails.
//!
//! Sends transactional emails via the Resend REST API.
//! Branding is configured via environment variables:
//!   APP_NAME      — application name (default: "Marshal")
//!   APP_DOMAIN    — public domain (default: "localhost")
//!   APP_COLOR     — brand color hex (default: "#7C83BC")
//!   RESEND_FROM_ADDRESS — override From header

use serde::Serialize;

/// Resend API email payload.
#[derive(Serialize)]
struct EmailPayload {
    from: String,
    to: Vec<String>,
    subject: String,
    html: String,
}

/// Resend email client.
pub struct ResendClient {
    api_key: String,
    from_address: String,
    http: reqwest::Client,
    app_name: String,
    domain: String,
    brand_color: String,
}

impl ResendClient {
    /// Create from environment variables.
    /// Returns `None` if `RESEND_API_KEY` is not set.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("RESEND_API_KEY").ok()?;
        if api_key.is_empty() || api_key == "REPLACE_ME" {
            tracing::warn!("RESEND_API_KEY is not configured — email sending disabled");
            return None;
        }
        let app_name = std::env::var("APP_NAME").unwrap_or_else(|_| "Marshal".into());
        let domain = std::env::var("APP_DOMAIN").unwrap_or_else(|_| "localhost".into());
        let brand_color = std::env::var("APP_COLOR").unwrap_or_else(|_| "#7C83BC".into());
        let from_address = std::env::var("RESEND_FROM_ADDRESS")
            .unwrap_or_else(|_| format!("{} <noreply@{}>", app_name, domain));
        tracing::info!("Resend email client initialized (from: {})", from_address);
        Some(Self {
            api_key,
            from_address,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            app_name,
            domain,
            brand_color,
        })
    }

    /// Send an email via Resend API.
    pub async fn send_email(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String> {
        let payload = EmailPayload {
            from: self.from_address.clone(),
            to: vec![to.to_string()],
            subject: subject.to_string(),
            html: html_body.to_string(),
        };

        let resp = self.http
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Resend request failed: {}", e))?;

        if resp.status().is_success() {
            tracing::info!("Email sent to {} — subject: {}", to, subject);
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("Resend API error {}: {}", status, body);
            Err(format!("Resend API error {}: {}", status, body))
        }
    }

    /// Send email verification link.
    pub async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        let link = format!("https://{}/verify-email?token={}", self.domain, token);
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:{color}">Verify Your Email</h2>
<p>Welcome to {app}! Please verify your email address by clicking the button below:</p>
<p style="text-align:center;margin:30px 0">
  <a href="{link}" style="background:{color};color:white;padding:12px 32px;border-radius:6px;text-decoration:none;font-weight:bold">Verify Email</a>
</p>
<p style="color:#666;font-size:14px">This link expires in 24 hours. If you didn't create an account, you can safely ignore this email.</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — AI Management Platform</p>
</div>"#,
            color = self.brand_color,
            app = self.app_name,
        );
        self.send_email(to, &format!("Verify your {} email", self.app_name), &html).await
    }

    /// Send welcome email after registration.
    pub async fn send_welcome_email(&self, to: &str, first_name: &str, org_name: &str) -> Result<(), String> {
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:{color}">Welcome to {app}, {first_name}!</h2>
<p>Your organization <strong>{org_name}</strong> has been created and your 90-day trial has started.</p>
<p>As the organization admin, you can:</p>
<ul>
<li>Invite team members and manage their permissions</li>
<li>Upload documents to your knowledge base</li>
<li>Set up workflows, timekeeping, and approvals</li>
<li>Use AI-powered Ask Marshal for answers from your documents</li>
</ul>
<p style="text-align:center;margin:30px 0">
  <a href="https://{domain}/marshal/" style="background:{color};color:white;padding:12px 32px;border-radius:6px;text-decoration:none;font-weight:bold">Go to Dashboard</a>
</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — Free Trial</p>
</div>"#,
            color = self.brand_color,
            app = self.app_name,
            domain = self.domain,
        );
        self.send_email(to, &format!("Welcome to {} — {} is ready", self.app_name, org_name), &html).await
    }

    /// Notify org admin of a new join request.
    pub async fn send_join_request_to_admin(
        &self,
        admin_email: &str,
        requester_name: &str,
        requester_email: &str,
        org_name: &str,
    ) -> Result<(), String> {
        let link = format!("https://{}/marshal/admin/join-requests", self.domain);
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:{color}">New Join Request</h2>
<p><strong>{requester_name}</strong> ({requester_email}) is requesting to join <strong>{org_name}</strong>.</p>
<p>Please review this request in your admin dashboard:</p>
<p style="text-align:center;margin:30px 0">
  <a href="{link}" style="background:{color};color:white;padding:12px 32px;border-radius:6px;text-decoration:none;font-weight:bold">Review Request</a>
</p>
<p style="color:#666;font-size:14px">This request expires in 7 days if no action is taken.</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — AI Management Platform</p>
</div>"#,
            color = self.brand_color,
            app = self.app_name,
        );
        self.send_email(admin_email, &format!("New join request for {} from {}", org_name, requester_name), &html).await
    }

    /// Notify user their join request was approved.
    pub async fn send_join_approved(&self, to: &str, first_name: &str, org_name: &str) -> Result<(), String> {
        let link = format!("https://{}/marshal/login", self.domain);
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:{color}">You're In, {first_name}!</h2>
<p>Your request to join <strong>{org_name}</strong> has been approved. You can now log in and start using the platform.</p>
<p style="text-align:center;margin:30px 0">
  <a href="{link}" style="background:{color};color:white;padding:12px 32px;border-radius:6px;text-decoration:none;font-weight:bold">Log In Now</a>
</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — AI Management Platform</p>
</div>"#,
            color = self.brand_color,
            app = self.app_name,
        );
        self.send_email(to, &format!("Welcome to {} on {}", org_name, self.app_name), &html).await
    }

    /// Notify user their join request was rejected.
    pub async fn send_join_rejected(&self, to: &str, first_name: &str, org_name: &str) -> Result<(), String> {
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:{color}">Join Request Update</h2>
<p>Hi {first_name}, your request to join <strong>{org_name}</strong> was not approved at this time.</p>
<p>If you believe this is an error, please contact the organization administrator directly.</p>
<p>You can also <a href="https://{domain}/marshal/register">create your own organization</a> to start a free trial.</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — AI Management Platform</p>
</div>"#,
            color = self.brand_color,
            app = self.app_name,
            domain = self.domain,
        );
        self.send_email(to, &format!("Update on your request to join {}", org_name), &html).await
    }

    /// Send trial expiration warning.
    pub async fn send_trial_warning(&self, to: &str, org_name: &str, days_remaining: i64) -> Result<(), String> {
        let urgency = if days_remaining <= 1 { "expires tomorrow" }
            else if days_remaining <= 3 { "expires in a few days" }
            else { "is expiring soon" };
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:#e67e22">Trial {urgency}</h2>
<p>Your trial for <strong>{org_name}</strong> has <strong>{days_remaining} day(s) remaining</strong>.</p>
<p>After expiration:</p>
<ul>
<li>You'll have 7 days of read-only access</li>
<li>After that, all access will be blocked</li>
<li>Data will be permanently deleted 30 days after expiration</li>
</ul>
<p>To continue, please contact our sales team about upgrading to a paid plan.</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — AI Management Platform</p>
</div>"#,
            app = self.app_name,
        );
        self.send_email(to, &format!("{} trial for {} — {} days remaining", self.app_name, org_name, days_remaining), &html).await
    }

    /// Send trial expired notice.
    pub async fn send_trial_expired(&self, to: &str, org_name: &str) -> Result<(), String> {
        let html = format!(
            r#"<div style="font-family:sans-serif;max-width:600px;margin:0 auto;padding:20px">
<h2 style="color:#e74c3c">Trial Expired</h2>
<p>Your trial for <strong>{org_name}</strong> has expired.</p>
<p>You have 7 days of read-only access remaining. After that, all data will be permanently deleted in 30 days.</p>
<p>To keep your data and continue, please contact sales to upgrade.</p>
<hr style="border:none;border-top:1px solid #eee;margin:30px 0">
<p style="color:#999;font-size:12px">{app} — AI Management Platform</p>
</div>"#,
            app = self.app_name,
        );
        self.send_email(to, &format!("{} trial for {} has expired", self.app_name, org_name), &html).await
    }
}
