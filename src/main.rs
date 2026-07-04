use trawler::Config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("trawler=info".parse().expect("valid tracing directive")),
        )
        .init();

    if let Err(err) = trawler::run(Config::default()).await {
        tracing::error!("fatal: {err}");
        std::process::exit(1);
    }
}
