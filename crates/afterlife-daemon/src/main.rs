use afterlife_daemon::{AppDaemonState, build_router};
use afterlife_protocol::AfterlifeConfig;

fn default_data_dir() -> String {
    if let Ok(dir) = std::env::var("AFTERLIFE_DATA_DIR") {
        return dir;
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/.afterlife");
    }
    ".afterlife".into()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].clone())
        .or_else(|| std::env::var("AFTERLIFE_CONFIG").ok());

    let config = match config_path {
        Some(path) => AfterlifeConfig::from_toml_file(&path).unwrap_or_else(|e| {
            eprintln!("[afterlifed] {e}");
            std::process::exit(1);
        }),
        None => AfterlifeConfig::local_default(&default_data_dir()),
    };

    let bind_addr = config.server.bind_addr.clone();
    let public_url = config.server.public_base_url.clone();
    let state = AppDaemonState::from_config(config).unwrap_or_else(|e| {
        eprintln!("[afterlifed] failed to initialize runtime ports: {e}");
        std::process::exit(1);
    });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("[afterlifed] failed to bind {bind_addr}: {e}");
            std::process::exit(1);
        });

    eprintln!("[afterlifed] listening on {bind_addr} (public: {public_url})");
    eprintln!("[afterlifed] REST: {public_url}/v1/workspace | SSE: {public_url}/v1/events | MCP: {public_url}/mcp");

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[afterlifed] server error: {e}");
        std::process::exit(1);
    }
}
