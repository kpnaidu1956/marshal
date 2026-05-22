//! Input validation for API security hardening
//!
//! Provides path traversal prevention, organization ID validation,
//! and input size limits to protect against common security issues.

use crate::error::{Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;

// ============================================================================
// Constants
// ============================================================================

/// Maximum length for organization ID (slug format)
pub const MAX_ORG_ID_LENGTH: usize = 128;

/// Maximum length for filenames
pub const MAX_FILENAME_LENGTH: usize = 255;

/// Maximum length for query strings
pub const MAX_QUERY_LENGTH: usize = 10_000;

/// Maximum length for error messages to prevent information leakage
pub const MAX_ERROR_MESSAGE_LENGTH: usize = 500;

// ============================================================================
// Regex Patterns (compiled once)
// ============================================================================

/// Organization ID format: lowercase alphanumeric with hyphens, no leading/trailing hyphens
/// Examples: "demo-org", "acme-corp-123", "organization1"
static ORG_ID_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-z0-9][a-z0-9\-]*[a-z0-9]$|^[a-z0-9]$").expect("Invalid org ID regex")
});

/// Safe filename pattern: alphanumeric, hyphens, underscores, dots, spaces
/// No path separators, no special characters
/// Note: Reserved for stricter filename validation in future
#[allow(dead_code)]
static SAFE_FILENAME_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9\-_\.\s]*[a-zA-Z0-9\.]$|^[a-zA-Z0-9]\.?[a-zA-Z0-9]*$")
        .expect("Invalid filename regex")
});

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate and sanitize organization ID
///
/// # Security
/// Prevents path traversal attacks via organization_id parameter.
/// Only allows lowercase alphanumeric characters and hyphens.
///
/// # Returns
/// - `Ok(())` if valid
/// - `Err(Error::Validation)` if invalid
pub fn validate_organization_id(org_id: &str) -> Result<()> {
    // Check for empty
    if org_id.is_empty() {
        return Err(Error::Validation("Organization ID is required".to_string()));
    }

    // Check length
    if org_id.len() > MAX_ORG_ID_LENGTH {
        return Err(Error::Validation(format!(
            "Organization ID exceeds maximum length of {} characters",
            MAX_ORG_ID_LENGTH
        )));
    }

    // Check for path traversal attempts
    if org_id.contains("..") || org_id.contains('/') || org_id.contains('\\') {
        return Err(Error::Validation(
            "Organization ID contains invalid path characters".to_string(),
        ));
    }

    // Check format
    if !ORG_ID_PATTERN.is_match(org_id) {
        return Err(Error::Validation(
            "Organization ID must be lowercase alphanumeric with hyphens (e.g., 'my-org-123')".to_string(),
        ));
    }

    Ok(())
}

/// Sanitize filename to prevent path traversal attacks
///
/// # Security
/// - Removes path separators (/ and \)
/// - Removes parent directory references (..)
/// - Strips leading/trailing whitespace
/// - Limits length
///
/// # Returns
/// Sanitized filename safe for file system operations
pub fn sanitize_filename(filename: &str) -> Result<String> {
    let filename = filename.trim();

    // Check for empty
    if filename.is_empty() {
        return Err(Error::Validation("Filename is required".to_string()));
    }

    // Check length before processing
    if filename.len() > MAX_FILENAME_LENGTH {
        return Err(Error::Validation(format!(
            "Filename exceeds maximum length of {} characters",
            MAX_FILENAME_LENGTH
        )));
    }

    // Check for path traversal attempts BEFORE extracting filename
    // This rejects inputs like "../../../etc/passwd"
    if filename.contains("..") {
        return Err(Error::Validation(
            "Filename contains invalid path traversal sequence".to_string(),
        ));
    }

    // Extract just the filename (remove any path components)
    let sanitized = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim();

    // Check for hidden files (starting with .)
    if sanitized.starts_with('.') && !sanitized.contains('.') {
        return Err(Error::Validation(
            "Hidden files without extension are not allowed".to_string(),
        ));
    }

    // Final validation
    if sanitized.is_empty() {
        return Err(Error::Validation("Filename is empty after sanitization".to_string()));
    }

    Ok(sanitized.to_string())
}

/// Validate query string length and content
///
/// # Security
/// Prevents DoS via extremely long query strings
pub fn validate_query(query: &str) -> Result<()> {
    if query.is_empty() {
        return Err(Error::Validation("Query cannot be empty".to_string()));
    }

    if query.len() > MAX_QUERY_LENGTH {
        return Err(Error::Validation(format!(
            "Query exceeds maximum length of {} characters",
            MAX_QUERY_LENGTH
        )));
    }

    Ok(())
}

/// Validate batch size for operations
///
/// # Security
/// Prevents resource exhaustion via large batch sizes
pub fn validate_batch_size(size: usize, max: usize) -> Result<()> {
    if size == 0 {
        return Err(Error::Validation("Batch size must be at least 1".to_string()));
    }

    if size > max {
        return Err(Error::Validation(format!(
            "Batch size {} exceeds maximum of {}",
            size, max
        )));
    }

    Ok(())
}

/// Validate pagination limit
///
/// # Security
/// Prevents memory exhaustion via unbounded result sets
pub fn validate_limit(limit: usize, max: usize) -> Result<usize> {
    if limit == 0 {
        return Ok(max.min(100)); // Default to reasonable limit
    }

    Ok(limit.min(max))
}

/// Truncate error message to prevent information leakage
///
/// # Security
/// Prevents leaking sensitive file paths, hashes, or internal details
pub fn sanitize_error_message(message: &str) -> String {
    if message.len() <= MAX_ERROR_MESSAGE_LENGTH {
        return message.to_string();
    }

    format!("{}...", &message[..MAX_ERROR_MESSAGE_LENGTH - 3])
}

/// Validate file size is within acceptable limits
///
/// # Security
/// Prevents memory exhaustion via large file uploads
pub fn validate_file_size(size: u64, max_bytes: u64) -> Result<()> {
    if size == 0 {
        return Err(Error::Validation("File is empty".to_string()));
    }

    if size > max_bytes {
        let max_mb = max_bytes / (1024 * 1024);
        let size_mb = size / (1024 * 1024);
        return Err(Error::Validation(format!(
            "File size ({} MB) exceeds maximum allowed ({} MB)",
            size_mb, max_mb
        )));
    }

    Ok(())
}

// ============================================================================
// Shared Query Types (extracted for DRY)
// ============================================================================

use serde::Deserialize;

/// Organization query parameter - shared across all endpoints requiring org context
#[derive(Debug, Clone, Deserialize)]
pub struct OrgQuery {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

impl OrgQuery {
    /// Validate the organization ID
    pub fn validate(&self) -> Result<()> {
        validate_organization_id(&self.organization_id)
    }

    /// Get validated organization ID
    pub fn validated_org_id(&self) -> Result<&str> {
        self.validate()?;
        Ok(&self.organization_id)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_org_ids() {
        assert!(validate_organization_id("demo-org").is_ok());
        assert!(validate_organization_id("acme-corp-123").is_ok());
        assert!(validate_organization_id("org1").is_ok());
        assert!(validate_organization_id("a").is_ok());
        assert!(validate_organization_id("a1").is_ok());
    }

    #[test]
    fn test_invalid_org_ids() {
        // Empty
        assert!(validate_organization_id("").is_err());

        // Path traversal
        assert!(validate_organization_id("../evil").is_err());
        assert!(validate_organization_id("org/subdir").is_err());
        assert!(validate_organization_id("org\\subdir").is_err());

        // Invalid characters
        assert!(validate_organization_id("ORG-UPPER").is_err()); // uppercase
        assert!(validate_organization_id("-starts-with-hyphen").is_err());
        assert!(validate_organization_id("ends-with-hyphen-").is_err());
        assert!(validate_organization_id("has spaces").is_err());
        assert!(validate_organization_id("has_underscore").is_err());
    }

    #[test]
    fn test_filename_sanitization() {
        // Valid filenames
        assert_eq!(sanitize_filename("document.pdf").unwrap(), "document.pdf");
        assert_eq!(sanitize_filename("my file.docx").unwrap(), "my file.docx");
        assert_eq!(sanitize_filename("file-name_v2.txt").unwrap(), "file-name_v2.txt");

        // Path traversal prevention
        assert_eq!(sanitize_filename("/path/to/file.pdf").unwrap(), "file.pdf");
        assert_eq!(sanitize_filename("C:\\Users\\doc.pdf").unwrap(), "doc.pdf");
        assert!(sanitize_filename("../../../etc/passwd").is_err());

        // Invalid
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("   ").is_err());
    }

    #[test]
    fn test_query_validation() {
        assert!(validate_query("what is the policy?").is_ok());
        assert!(validate_query("").is_err());

        // Long query should fail
        let long_query = "a".repeat(MAX_QUERY_LENGTH + 1);
        assert!(validate_query(&long_query).is_err());
    }

    #[test]
    fn test_batch_size_validation() {
        assert!(validate_batch_size(1, 100).is_ok());
        assert!(validate_batch_size(100, 100).is_ok());
        assert!(validate_batch_size(0, 100).is_err());
        assert!(validate_batch_size(101, 100).is_err());
    }
}
