use crate::config::Config;

#[derive(Clone, Debug)]
pub struct AppState {
    pub config: Config,
    pub version: &'static str,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}
