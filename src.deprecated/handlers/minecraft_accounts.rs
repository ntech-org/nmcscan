use crate::handlers::{AppState, CreateMinecraftAccountRequest, MinecraftAccountResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};

pub async fn list_accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<MinecraftAccountResponse>>, StatusCode> {
    let accounts: Vec<crate::models::entities::minecraft_accounts::Model> = state
        .minecraft_account_repo
        .get_all_accounts()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = accounts
        .into_iter()
        .map(|a| MinecraftAccountResponse {
            id: a.id,
            email: a.email,
            status: a.status,
            expires_at: a.expires_at,
        })
        .collect();

    Ok(Json(response))
}

pub async fn add_account(
    State(state): State<AppState>,
    Json(req): Json<CreateMinecraftAccountRequest>,
) -> Result<Json<MinecraftAccountResponse>, StatusCode> {
    let account: crate::models::entities::minecraft_accounts::Model = state
        .minecraft_account_repo
        .add_account(req.email, req.password, req.access_token, None, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MinecraftAccountResponse {
        id: account.id,
        email: account.email,
        status: account.status,
        expires_at: account.expires_at,
    }))
}

pub async fn delete_account(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let _: sea_orm::DeleteResult = state
        .minecraft_account_repo
        .delete_account(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
