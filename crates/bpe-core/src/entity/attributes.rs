use crate::entity::models::{FieldDef, FieldType};
use crate::error::BpeError;

/// Validate entity attributes against field definitions.
pub fn validate_attributes(
    fields: &[FieldDef],
    attributes: &serde_json::Value,
) -> Result<(), BpeError> {
    let attrs = attributes
        .as_object()
        .ok_or_else(|| BpeError::BadRequest("attributes must be a JSON object".into()))?;

    for field in fields {
        let value = attrs.get(&field.name);

        // Check required fields
        if field.required {
            match value {
                None | Some(serde_json::Value::Null) => {
                    return Err(BpeError::BadRequest(format!(
                        "Required field '{}' ({}) is missing",
                        field.name, field.label
                    )));
                }
                _ => {}
            }
        }

        // Validate type if value is present and non-null
        if let Some(val) = value {
            if !val.is_null() {
                validate_field_type(&field.name, &field.field_type, val)?;
            }
        }
    }

    Ok(())
}

fn validate_field_type(name: &str, field_type: &FieldType, value: &serde_json::Value) -> Result<(), BpeError> {
    match field_type {
        FieldType::String | FieldType::Text => {
            if !value.is_string() {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be a string")));
            }
        }
        FieldType::Integer => {
            if !value.is_i64() && !value.is_u64() {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be an integer")));
            }
        }
        FieldType::Decimal => {
            if !value.is_f64() && !value.is_i64() {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be a number")));
            }
        }
        FieldType::Boolean => {
            if !value.is_boolean() {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be a boolean")));
            }
        }
        FieldType::Date => {
            let s = value.as_str().ok_or_else(|| BpeError::BadRequest(format!("Field '{name}' must be a date string (YYYY-MM-DD)")))?;
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| BpeError::BadRequest(format!("Field '{name}' must be YYYY-MM-DD format")))?;
        }
        FieldType::DateTime => {
            let s = value.as_str().ok_or_else(|| BpeError::BadRequest(format!("Field '{name}' must be a datetime string")))?;
            chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|_| BpeError::BadRequest(format!("Field '{name}' must be ISO 8601 format")))?;
        }
        FieldType::Email => {
            let s = value.as_str().ok_or_else(|| BpeError::BadRequest(format!("Field '{name}' must be an email string")))?;
            if !s.contains('@') || !s.contains('.') {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be a valid email")));
            }
        }
        FieldType::Phone => {
            let s = value.as_str().ok_or_else(|| BpeError::BadRequest(format!("Field '{name}' must be a phone string")))?;
            if !s.starts_with('+') {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be E.164 format (e.g. +61...)")));
            }
        }
        FieldType::Uuid => {
            let s = value.as_str().ok_or_else(|| BpeError::BadRequest(format!("Field '{name}' must be a UUID string")))?;
            uuid::Uuid::parse_str(s)
                .map_err(|_| BpeError::BadRequest(format!("Field '{name}' must be a valid UUID")))?;
        }
        FieldType::Currency => {
            let obj = value.as_object().ok_or_else(|| {
                BpeError::BadRequest(format!("Field '{name}' must be {{ \"amount\": number, \"currency\": \"AUD\" }}"))
            })?;
            if !obj.contains_key("amount") || !obj.contains_key("currency") {
                return Err(BpeError::BadRequest(format!("Field '{name}' must have 'amount' and 'currency' keys")));
            }
        }
        FieldType::Enum(variants) => {
            let s = value.as_str().ok_or_else(|| BpeError::BadRequest(format!("Field '{name}' must be a string")))?;
            if !variants.iter().any(|v| v == s) {
                return Err(BpeError::BadRequest(format!(
                    "Field '{name}' must be one of: {}",
                    variants.join(", ")
                )));
            }
        }
        FieldType::JsonObject => {
            if !value.is_object() {
                return Err(BpeError::BadRequest(format!("Field '{name}' must be a JSON object")));
            }
        }
    }

    Ok(())
}
