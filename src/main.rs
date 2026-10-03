use std::net::SocketAddr;

use expo_trace::{db, router, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/expo.db?mode=rwc".into());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let upload_dir = std::path::PathBuf::from(
        std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "data/uploads".into()),
    );
    tokio::fs::create_dir_all(&upload_dir).await?;
    tokio::fs::create_dir_all("data").await?;

    let pool = db::connect(&db_url).await?;
    db::migrate(&pool).await?;
    db::seed(&pool).await?;

    let app = router(AppState { pool, upload_dir });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("展会展品入馆封箱与撤展核对系统已启动: http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
