use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use anyhow::Context;
use rinha_backend_2026_luckstai_rust::config::AppConfig;
use rinha_backend_2026_luckstai_rust::detector::Detector;
use rinha_backend_2026_luckstai_rust::domain::FraudRequest;
use std::sync::Arc;

struct AppState {
    detector: Arc<Detector>,
}

const FRAUD_RESPONSES: [&[u8]; 6] = [
    br#"{"approved":true,"fraud_score":0.0}"#,
    br#"{"approved":true,"fraud_score":0.2}"#,
    br#"{"approved":true,"fraud_score":0.4}"#,
    br#"{"approved":false,"fraud_score":0.6}"#,
    br#"{"approved":false,"fraud_score":0.8}"#,
    br#"{"approved":false,"fraud_score":1.0}"#,
];

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = AppConfig::from_env();
    let detector = Arc::new(
        Detector::load(&config)
            .with_context(|| "failed to initialize detector")
            .map_err(to_io_error)?,
    );

    let app_state = web::Data::new(AppState {
        detector: detector.clone(),
    });

    eprintln!(
        "detector ready with {} references on {}",
        detector.reference_count(),
        config.bind_addr
    );

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/ready", web::get().to(ready))
            .route("/fraud-score", web::post().to(fraud_score))
    })
    .workers(config.workers)
    .bind(&config.bind_addr)?
    .run()
    .await
}

async fn ready() -> impl Responder {
    HttpResponse::Ok().finish()
}

async fn fraud_score(
    state: web::Data<AppState>,
    payload: web::Bytes,
) -> actix_web::Result<impl Responder> {
    let request: FraudRequest = serde_json::from_slice(&payload)
        .map_err(actix_web::error::ErrorBadRequest)?;
    let fraud_neighbors = state
        .detector
        .fraud_neighbors(&request)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = FRAUD_RESPONSES
        .get(fraud_neighbors as usize)
        .copied()
        .ok_or_else(|| actix_web::error::ErrorInternalServerError("invalid fraud count"))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(body))
}

fn to_io_error(error: anyhow::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
