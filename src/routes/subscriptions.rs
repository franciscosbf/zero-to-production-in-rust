use actix_web::{web, HttpResponse};

#[derive(serde::Deserialize)]
#[allow(dead_code)] // TODO: remove
pub struct FormData {
    email: String,
    name: String,
}

pub async fn subscribe(_form: web::Form<FormData>) -> HttpResponse {
    HttpResponse::Ok().finish()
}
