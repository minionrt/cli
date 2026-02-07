use actix_web::{get, HttpResponse, Responder};

#[get("/ready")]
async fn readiness() -> impl Responder {
    HttpResponse::Ok().body("Ready")
}

#[get("/healthz")]
async fn healthz() -> impl Responder {
    HttpResponse::Ok().body("Healthy")
}
