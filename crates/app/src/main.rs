use anyhow::Result;
use std::sync::Arc;
use tracing::info;

mod analysis;
mod config;
mod github;
mod http;
mod observability;
mod store;
mod worker;

use config::Config;
use store::db;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    observability::init();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let config = Arc::new(config);
    info!(mode = %config.mode, "Starting awaitguard");

    match config.mode.as_str() {
        "api" => {
            http::serve(config, pool).await?;
        }
        "worker" => {
            worker::runner::run(config, pool).await?;
        }
        _ => {
            let config2 = Arc::clone(&config);
            let pool2 = pool.clone();
            let api = tokio::spawn(async move { http::serve(config2, pool2).await });
            let wrk = tokio::spawn(async move { worker::runner::run(config, pool).await });
            tokio::select! {
                r = api => { r?? }
                r = wrk => { r?? }
            }
        }
    }

    Ok(())
}
