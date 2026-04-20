use actix_web::{HttpRequest, HttpResponse, web};
use reqwest::Client;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
];

pub async fn forward(
    req: HttpRequest,
    body: web::Bytes,
    upstream: &str,
    client: &Client,
) -> Result<HttpResponse, actix_web::Error> {
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let upstream_url = format!("{upstream}{path}");

    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes())
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

    let mut builder = client.request(method, &upstream_url);

    for (name, value) in req.headers() {
        let lower = name.as_str().to_lowercase();
        if lower != "host" && !HOP_BY_HOP.contains(&lower.as_str()) {
            builder = builder.header(name.as_str(), value.as_bytes());
        }
    }

    if let Some(peer) = req.peer_addr() {
        builder = builder.header("X-Forwarded-For", peer.ip().to_string());
    }
    builder = builder.header(
        "X-Forwarded-Proto",
        req.connection_info().scheme().to_string(),
    );

    let upstream_resp = builder
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| actix_web::error::ErrorBadGateway(e.to_string()))?;

    let status = actix_web::http::StatusCode::from_u16(upstream_resp.status().as_u16())
        .expect("reqwest status is always a valid u16");

    let mut response = HttpResponse::build(status);
    for (name, value) in upstream_resp.headers() {
        let lower = name.as_str().to_lowercase();
        if !HOP_BY_HOP.contains(&lower.as_str()) {
            response.insert_header((name.as_str(), value.as_bytes()));
        }
    }

    Ok(response.streaming(upstream_resp.bytes_stream()))
}
