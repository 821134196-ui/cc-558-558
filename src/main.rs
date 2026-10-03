use expo_guard::{build_router, db, AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/app.db?mode=rwc".to_string());
    let upload_dir = std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "uploads".to_string());
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    std::fs::create_dir_all("data").ok();
    std::fs::create_dir_all(&upload_dir).ok();

    let pool = db::init_pool(&db_url, 5).await.expect("数据库初始化失败");
    let app = build_router(AppState { pool, upload_dir });

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("端口绑定失败");
    tracing::info!("展会展品核对系统已启动: http://localhost:3000");
    axum::serve(listener, app).await.expect("服务运行失败");
}
