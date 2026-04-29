//! SQLite persistence for Web-configurable LLM providers.

use crate::error::{GscholarError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertProvider {
    pub name: String,
    pub enabled: bool,
    pub interface_type: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub order: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicProvider {
    pub name: String,
    pub enabled: bool,
    pub interface_type: String,
    pub endpoint: String,
    pub model: String,
    pub api_key_set: bool,
    #[serde(rename = "order")]
    pub order: i64,
}

#[derive(Debug, Clone)]
pub struct RuntimeProvider {
    pub name: String,
    pub enabled: bool,
    pub interface_type: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub order: i64,
}

fn normalize_name(name: &str) -> Result<String> {
    let normalized = name.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(GscholarError::Validation(
            "LLM provider name is required".to_string(),
        ));
    }
    Ok(normalized)
}

fn validate_common(provider: &UpsertProvider) -> Result<()> {
    normalize_name(&provider.name)?;
    if provider.endpoint.trim().is_empty() {
        return Err(GscholarError::Validation(
            "LLM provider endpoint is required".to_string(),
        ));
    }
    if provider.model.trim().is_empty() {
        return Err(GscholarError::Validation(
            "LLM provider model is required".to_string(),
        ));
    }
    crate::llm::openai_compatible::InterfaceType::parse(&provider.interface_type)?;
    Ok(())
}

/// Upsert a provider. `api_key = None` or blank preserves the existing key.
pub fn upsert(conn: &Connection, provider: UpsertProvider) -> Result<()> {
    validate_common(&provider)?;
    let name = normalize_name(&provider.name)?;
    let interface_type =
        crate::llm::openai_compatible::InterfaceType::parse(&provider.interface_type)?;
    let endpoint = crate::llm::openai_compatible::normalize_endpoint_for_interface(
        &provider.endpoint,
        interface_type,
    );
    let now = chrono::Utc::now().timestamp();
    let existing_key: Option<String> = conn
        .query_row(
            "SELECT api_key FROM llm_providers WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| GscholarError::Database(format!("Read LLM provider failed: {}", e)))?;

    let api_key = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or(existing_key)
        .unwrap_or_default();

    conn.execute(
        r#"
        INSERT INTO llm_providers
            (name, enabled, interface_type, endpoint, model, api_key, provider_order, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
        ON CONFLICT(name) DO UPDATE SET
            enabled = excluded.enabled,
            interface_type = excluded.interface_type,
            endpoint = excluded.endpoint,
            model = excluded.model,
            api_key = excluded.api_key,
            provider_order = excluded.provider_order,
            updated_at = excluded.updated_at
        "#,
        params![
            name,
            provider.enabled as i32,
            provider.interface_type.trim(),
            endpoint,
            provider.model.trim(),
            api_key,
            provider.order,
            now,
        ],
    )
    .map_err(|e| GscholarError::Database(format!("Upsert LLM provider failed: {}", e)))?;

    Ok(())
}

pub fn list_public(conn: &Connection) -> Result<Vec<PublicProvider>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT name, enabled, interface_type, endpoint, model,
                   CASE WHEN length(trim(api_key)) > 0 THEN 1 ELSE 0 END AS api_key_set,
                   provider_order
            FROM llm_providers
            ORDER BY provider_order ASC, name ASC
            "#,
        )
        .map_err(|e| GscholarError::Database(format!("List LLM providers failed: {}", e)))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(PublicProvider {
                name: row.get(0)?,
                enabled: row.get::<_, i32>(1)? != 0,
                interface_type: row.get(2)?,
                endpoint: row.get(3)?,
                model: row.get(4)?,
                api_key_set: row.get::<_, i32>(5)? != 0,
                order: row.get(6)?,
            })
        })
        .map_err(|e| GscholarError::Database(format!("Map LLM providers failed: {}", e)))?;

    Ok(rows.filter_map(|row| row.ok()).collect())
}

pub fn list_enabled_runtime(conn: &Connection) -> Result<Vec<RuntimeProvider>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT name, enabled, interface_type, endpoint, model, api_key, provider_order
            FROM llm_providers
            WHERE enabled = 1 AND length(trim(api_key)) > 0
            ORDER BY provider_order ASC, name ASC
            "#,
        )
        .map_err(|e| {
            GscholarError::Database(format!("List runtime LLM providers failed: {}", e))
        })?;

    let rows = stmt
        .query_map([], runtime_from_row)
        .map_err(|e| GscholarError::Database(format!("Map runtime LLM providers failed: {}", e)))?;

    Ok(rows.filter_map(|row| row.ok()).collect())
}

pub fn get_runtime(conn: &Connection, name: &str) -> Result<Option<RuntimeProvider>> {
    let name = normalize_name(name)?;
    conn.query_row(
        r#"
        SELECT name, enabled, interface_type, endpoint, model, api_key, provider_order
        FROM llm_providers
        WHERE name = ?1
        "#,
        params![name],
        runtime_from_row,
    )
    .optional()
    .map_err(|e| GscholarError::Database(format!("Get runtime LLM provider failed: {}", e)))
}

pub fn delete(conn: &Connection, name: &str) -> Result<bool> {
    let name = normalize_name(name)?;
    let affected = conn
        .execute("DELETE FROM llm_providers WHERE name = ?1", params![name])
        .map_err(|e| GscholarError::Database(format!("Delete LLM provider failed: {}", e)))?;
    Ok(affected > 0)
}

pub fn has_any(conn: &Connection) -> Result<bool> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM llm_providers", [], |row| row.get(0))
        .map_err(|e| GscholarError::Database(format!("Count LLM providers failed: {}", e)))?;
    Ok(count > 0)
}

fn runtime_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeProvider> {
    Ok(RuntimeProvider {
        name: row.get(0)?,
        enabled: row.get::<_, i32>(1)? != 0,
        interface_type: row.get(2)?,
        endpoint: row.get(3)?,
        model: row.get(4)?,
        api_key: row.get(5)?,
        order: row.get(6)?,
    })
}
