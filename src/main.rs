mod helpers;
mod scrapers;
mod server;

use actix_web::{rt::spawn, rt::task::spawn_blocking, rt::time::sleep};
use log::{error, info, LevelFilter};
use std::time::Duration;

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();

    spawn(periodic_get_recent_posts());
    server::run().await?;

    Ok(())
}

async fn periodic_get_recent_posts() {
    loop {
        let result = spawn_blocking(|| scrapers::get_recent_posts()).await;

        match result {
            Ok(Ok(_)) => info!("Successfully ran get_recent_posts"),
            Ok(Err(e)) => error!("Error running get_recent_posts: {:?}", e),
            Err(e) => error!("Failed to spaen blocking task: {:?}", e),
        }
        sleep(Duration::from_secs(3600)).await;
    }
}
