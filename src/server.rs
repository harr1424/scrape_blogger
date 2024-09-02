use actix_files::NamedFile;
use actix_route_rate_limiter::{LimiterBuilder, RateLimiter};
use actix_web::middleware::Logger;
use actix_web::{get, App, HttpServer, Result};
use chrono::Duration;
use std::path::PathBuf;
use std::sync::Arc;

#[get("/")]
async fn serve_file() -> Result<NamedFile> {
    let path: PathBuf = PathBuf::from("recents.json");
    Ok(NamedFile::open(path)?)
}

pub async fn run() -> std::io::Result<()> {
    let limiter = LimiterBuilder::new()
        .with_duration(Duration::minutes(15))
        .with_num_requests(30)
        .build();

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(RateLimiter::new(Arc::clone(&limiter)))
            .service(serve_file)
    })
    .bind("0.0.0.0:3333")?
    .run()
    .await
}
