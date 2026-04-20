use crate::models::{AdminToken, CreateTokenRequest, CreateTokenResponse, TokenInfo, TokenRecord};
use crate::store::TokenStore;
use actix_web::{HttpRequest, HttpResponse, web};
use std::sync::Arc;
use uuid::Uuid;

pub type SharedStore = Arc<TokenStore>;

pub async fn create_token(
    req: HttpRequest,
    body: web::Json<CreateTokenRequest>,
    store: web::Data<SharedStore>,
    admin_token: web::Data<AdminToken>,
) -> HttpResponse {
    if !is_admin(&req, &admin_token.get_ref().0) {
        return HttpResponse::Forbidden().finish();
    }

    if body.name.is_empty() || body.name == "__admin__" || body.name.contains(':') {
        return HttpResponse::BadRequest().body("invalid token name");
    }
    if body.namespaces.is_empty() {
        return HttpResponse::BadRequest().body("namespaces cannot be empty");
    }
    if body.namespaces.iter().any(|ns| ns == "*") {
        return HttpResponse::BadRequest().body("wildcard namespace not permitted via API");
    }

    let token = Uuid::new_v4().to_string();
    let record = TokenRecord {
        name: body.name.clone(),
        token: token.clone(),
        namespaces: body.namespaces.clone(),
    };

    let store_clone = Arc::clone(store.get_ref());
    let result =
        tokio::task::spawn_blocking(move || store_clone.insert(&record).map_err(|e| e.to_string()))
            .await;

    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(CreateTokenResponse { token }),
        Ok(Err(e)) => HttpResponse::InternalServerError().body(e),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn list_tokens(
    req: HttpRequest,
    store: web::Data<SharedStore>,
    admin_token: web::Data<AdminToken>,
) -> HttpResponse {
    if !is_admin(&req, &admin_token.get_ref().0) {
        return HttpResponse::Forbidden().finish();
    }

    let store_clone = Arc::clone(store.get_ref());
    let result =
        tokio::task::spawn_blocking(move || store_clone.list_all().map_err(|e| e.to_string()))
            .await;

    match result {
        Ok(Ok(records)) => {
            let infos: Vec<TokenInfo> = records
                .into_iter()
                .filter(|r| r.name != "__admin__")
                .map(|r| TokenInfo {
                    name: r.name,
                    namespaces: r.namespaces,
                })
                .collect();
            HttpResponse::Ok().json(infos)
        }
        Ok(Err(e)) => HttpResponse::InternalServerError().body(e),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn delete_token(
    req: HttpRequest,
    path: web::Path<String>,
    store: web::Data<SharedStore>,
    admin_token: web::Data<AdminToken>,
) -> HttpResponse {
    if !is_admin(&req, &admin_token.get_ref().0) {
        return HttpResponse::Forbidden().finish();
    }

    let name = path.into_inner();
    if name == "__admin__" {
        return HttpResponse::Forbidden().body("cannot delete admin token via API");
    }

    let store_clone = Arc::clone(store.get_ref());
    let result = tokio::task::spawn_blocking(move || {
        store_clone.delete_by_name(&name).map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(true)) => HttpResponse::Ok().finish(),
        Ok(Ok(false)) => HttpResponse::NotFound().finish(),
        Ok(Err(e)) => HttpResponse::InternalServerError().body(e),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

fn is_admin(req: &HttpRequest, admin_token: &str) -> bool {
    extract_bearer(req)
        .map(|t| ct_eq(&t, admin_token))
        .unwrap_or(false)
}

/// Constant-time string comparison — prevents timing attacks on the admin token.
fn ct_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Extracts the Bearer token from Authorization header (case-insensitive scheme per RFC 6750).
pub fn extract_bearer(req: &HttpRequest) -> Option<String> {
    let value = req.headers().get("Authorization")?.to_str().ok()?;
    if value.len() >= 7 && value[..7].eq_ignore_ascii_case("bearer ") {
        Some(value[7..].to_string())
    } else {
        None
    }
}
