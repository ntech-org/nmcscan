use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::handlers::{AppState, AuthContext};

#[derive(Serialize)]
pub struct ApiKeyResponse {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
}

pub async fn list_keys(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<ApiKeyResponse>>, StatusCode> {
    let user_id = auth.user_id.ok_or(StatusCode::UNAUTHORIZED)?;

    let keys = state
        .api_key_repo
        .list_for_user(user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list API keys: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let response = keys
        .into_iter()
        .map(|k| ApiKeyResponse {
            id: k.id,
            name: k.name,
            key: None, // Never return the key after creation
            created_at: k.created_at.into(),
            last_used_at: k.last_used_at.map(|t| t.into()),
        })
        .collect();

    Ok(Json(response))
}

pub async fn create_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(payload): Json<CreateKeyRequest>,
) -> Result<Json<ApiKeyResponse>, StatusCode> {
    let user_id = auth.user_id.ok_or(StatusCode::UNAUTHORIZED)?;

    if payload.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let raw_key = format!("nmc_{}", Uuid::new_v4().simple());

    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let key_hash = format!("{:x}", hasher.finalize());

    let key_model = state
        .api_key_repo
        .create_key(user_id, &payload.name, &key_hash)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create API key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ApiKeyResponse {
        id: key_model.id,
        name: key_model.name,
        key: Some(raw_key), // Only returned once
        created_at: key_model.created_at.into(),
        last_used_at: None,
    }))
}

pub async fn revoke_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = auth.user_id.ok_or(StatusCode::UNAUTHORIZED)?;

    let success = state
        .api_key_repo
        .revoke_key(user_id, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to revoke API key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if success {
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
