use axum::{Extension, Json};
use serde_json::{json, Value};
use crate::{
    auth::{RequireClaims, RevokedTokens},
    error::Result,
};

pub async fn logout(
    Extension(revoked): Extension<RevokedTokens>,
    RequireClaims(claims): RequireClaims,
) -> Result<Json<Value>> {
    revoked.lock().unwrap().insert(claims.jti);
    Ok(Json(json!({ "ok": true })))
}
