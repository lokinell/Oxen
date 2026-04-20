use crate::controllers::tokens;
use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/hub/tokens")
            .route("", web::post().to(tokens::create_token))
            .route("", web::get().to(tokens::list_tokens))
            .route("/{name}", web::delete().to(tokens::delete_token)),
    );
}
