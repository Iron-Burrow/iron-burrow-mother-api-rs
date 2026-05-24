use std::{env, net::SocketAddr};

const DEFAULT_APP_ENV: &str = "development";
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: u16 = 3000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub app_env: String,
    pub http_host: String,
    pub http_port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            app_env: env::var("APP_ENV").unwrap_or_else(|_| DEFAULT_APP_ENV.to_string()),
            http_host: env::var("HTTP_HOST").unwrap_or_else(|_| DEFAULT_HTTP_HOST.to_string()),
            http_port: match env::var("HTTP_PORT") {
                Ok(value) => value
                    .parse()
                    .map_err(|_| ConfigError::InvalidHttpPort(value))?,
                Err(_) => DEFAULT_HTTP_PORT,
            },
        })
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.http_host, self.http_port)
            .parse()
            .map_err(|_| ConfigError::InvalidSocketAddress {
                host: self.http_host.clone(),
                port: self.http_port,
            })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_env: DEFAULT_APP_ENV.to_string(),
            http_host: DEFAULT_HTTP_HOST.to_string(),
            http_port: DEFAULT_HTTP_PORT,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ConfigError {
    InvalidHttpPort(String),
    InvalidSocketAddress { host: String, port: u16 },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHttpPort(value) => {
                write!(formatter, "HTTP_PORT must be a valid u16, got {value:?}")
            }
            Self::InvalidSocketAddress { host, port } => {
                write!(
                    formatter,
                    "HTTP_HOST and HTTP_PORT must form a valid socket address, got {host}:{port}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_public_contract() {
        let config = Config::default();

        assert_eq!(config.app_env, "development");
        assert_eq!(config.http_host, "0.0.0.0");
        assert_eq!(config.http_port, 3000);
        assert_eq!(
            config.socket_addr().unwrap(),
            "0.0.0.0:3000".parse::<SocketAddr>().unwrap()
        );
    }
}
