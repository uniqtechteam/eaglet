//! The shop server: `cargo run -p shop`, then open <http://localhost:3000/docs>.

use std::net::SocketAddr;

use eaglet::App;
use shop::app_module::AppModule;
use shop::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = Config::from_env();
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = App::new::<AppModule>()
        .await?
        .with_info("Shop API", env!("CARGO_PKG_VERSION"))
        .swagger("/docs");
    tracing::info!("listening on http://{addr}, API docs at /docs");
    app.listen(addr).await?;
    Ok(())
}
