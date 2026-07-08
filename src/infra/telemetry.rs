pub(crate) fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "iron_burrow_mother_api_rs=info,tower_http=info".into());

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .init();
}
