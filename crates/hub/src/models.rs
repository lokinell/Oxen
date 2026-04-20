use serde::{Deserialize, Serialize};

pub struct AdminToken(pub String);
pub struct UpstreamUrl(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub name: String,
    pub token: String,
    pub namespaces: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub namespaces: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenInfo {
    pub name: String,
    pub namespaces: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub token: String,
}
