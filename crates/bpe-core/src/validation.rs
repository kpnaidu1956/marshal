//! Input validation utilities for BPE.

use crate::error::BpeError;

/// Maximum length for name fields (workflow names, entity names, etc.)
const MAX_NAME_LENGTH: usize = 300;
/// Maximum length for description fields.
const MAX_DESCRIPTION_LENGTH: usize = 5000;
/// Maximum length for category fields.
const MAX_CATEGORY_LENGTH: usize = 100;
/// Maximum page size for pagination.
pub const MAX_PAGE_SIZE: i64 = 200;
/// Default page size.
pub const DEFAULT_PAGE_SIZE: i64 = 50;

/// Validate a name field — non-empty, within length limits, no control characters.
pub fn validate_name(field: &str, value: &str) -> Result<(), BpeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(BpeError::BadRequest(format!("{field} cannot be empty")));
    }
    if trimmed.len() > MAX_NAME_LENGTH {
        return Err(BpeError::BadRequest(format!(
            "{field} exceeds maximum length of {MAX_NAME_LENGTH} characters"
        )));
    }
    if trimmed.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(BpeError::BadRequest(format!(
            "{field} contains invalid control characters"
        )));
    }
    Ok(())
}

/// Validate an optional description field.
pub fn validate_description(value: Option<&str>) -> Result<(), BpeError> {
    if let Some(desc) = value {
        if desc.len() > MAX_DESCRIPTION_LENGTH {
            return Err(BpeError::BadRequest(format!(
                "description exceeds maximum length of {MAX_DESCRIPTION_LENGTH} characters"
            )));
        }
    }
    Ok(())
}

/// Validate a category field.
pub fn validate_category(value: &str) -> Result<(), BpeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(BpeError::BadRequest("category cannot be empty".into()));
    }
    if trimmed.len() > MAX_CATEGORY_LENGTH {
        return Err(BpeError::BadRequest(format!(
            "category exceeds maximum length of {MAX_CATEGORY_LENGTH} characters"
        )));
    }
    // Category should be alphanumeric + hyphens/underscores
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(BpeError::BadRequest(
            "category may only contain letters, numbers, hyphens, underscores, and spaces".into(),
        ));
    }
    Ok(())
}

/// Validate an organization slug.
pub fn validate_org_slug(slug: &str) -> Result<(), BpeError> {
    if slug.is_empty() || slug.len() > 100 {
        return Err(BpeError::BadRequest("organization_id is invalid".into()));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(BpeError::BadRequest(
            "organization_id must contain only lowercase letters, digits, and hyphens".into(),
        ));
    }
    Ok(())
}

/// Cap and validate pagination parameters.
pub fn validate_pagination(page: Option<i64>, per_page: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let per_page = per_page.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    (page, per_page)
}

/// Validate SQL template more strictly.
pub fn validate_sql_template(sql: &str) -> Result<(), BpeError> {
    if sql.trim().is_empty() {
        return Err(BpeError::BadRequest("sql_template cannot be empty".into()));
    }
    if sql.len() > 10_000 {
        return Err(BpeError::BadRequest(
            "sql_template exceeds maximum length of 10000 characters".into(),
        ));
    }

    let upper = sql.to_uppercase();

    // Must start with SELECT or WITH (CTEs)
    let trimmed = upper.trim_start();
    if !trimmed.starts_with("SELECT") && !trimmed.starts_with("WITH") {
        return Err(BpeError::BadRequest(
            "Report SQL must start with SELECT or WITH".into(),
        ));
    }

    // Forbidden keywords
    let forbidden = [
        "INSERT ", "UPDATE ", "DELETE ", "DROP ", "ALTER ", "TRUNCATE ",
        "CREATE ", "GRANT ", "REVOKE ", "COPY ", "EXECUTE ", "CALL ",
        "SET ", "DO ", "PERFORM ", "LISTEN ", "NOTIFY ", "LOAD ",
    ];
    for kw in &forbidden {
        if upper.contains(kw) {
            return Err(BpeError::BadRequest(format!(
                "Report SQL must be read-only. Forbidden keyword: {}",
                kw.trim()
            )));
        }
    }

    // Forbid semicolons (prevent multi-statement injection)
    if sql.contains(';') {
        return Err(BpeError::BadRequest(
            "Report SQL must not contain semicolons".into(),
        ));
    }

    // Forbid comment markers that could hide injection
    if sql.contains("--") || sql.contains("/*") {
        return Err(BpeError::BadRequest(
            "Report SQL must not contain SQL comments".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name() {
        assert!(validate_name("name", "Hello").is_ok());
        assert!(validate_name("name", "").is_err());
        assert!(validate_name("name", "   ").is_err());
        assert!(validate_name("name", &"x".repeat(301)).is_err());
    }

    #[test]
    fn test_validate_category() {
        assert!(validate_category("hr").is_ok());
        assert!(validate_category("my-category").is_ok());
        assert!(validate_category("").is_err());
        assert!(validate_category("bad;cat").is_err());
    }

    #[test]
    fn test_validate_org_slug() {
        assert!(validate_org_slug("acme-corp").is_ok());
        assert!(validate_org_slug("UPPER").is_err());
        assert!(validate_org_slug("").is_err());
    }

    #[test]
    fn test_validate_sql_template() {
        assert!(validate_sql_template("SELECT count(*) FROM bpe.entities").is_ok());
        assert!(validate_sql_template("WITH cte AS (SELECT 1) SELECT * FROM cte").is_ok());
        assert!(validate_sql_template("DELETE FROM bpe.entities").is_err());
        assert!(validate_sql_template("SELECT 1; DROP TABLE bpe.entities").is_err());
        assert!(validate_sql_template("SELECT 1 -- comment").is_err());
        assert!(validate_sql_template("").is_err());
    }

    #[test]
    fn test_validate_pagination() {
        assert_eq!(validate_pagination(None, None), (1, 50));
        assert_eq!(validate_pagination(Some(0), Some(500)), (1, 200));
        assert_eq!(validate_pagination(Some(3), Some(10)), (3, 10));
    }
}
