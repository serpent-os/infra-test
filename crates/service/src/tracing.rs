//! Tracing support
use std::env;

use serde::Deserialize;
use tracing_subscriber::EnvFilter;

/// Output format
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Compact
    #[default]
    Compact,
    /// JSON
    Json,
}

/// Tracing configuration
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Level filter, such as `my_crate=info,my_crate::my_mod=debug,[my_span]=trace`
    #[serde(default = "default_level_filter")]
    pub level_filter: String,
    /// Output format
    #[serde(default)]
    pub format: Format,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level_filter: default_level_filter(),
            format: Format::default(),
        }
    }
}

fn default_level_filter() -> String {
    "info".into()
}

/// Initialize tracing using the provided [`Config`]
///
/// `RUST_LOG` env var can be set at runtime to override the [`Config::level_filter`]
pub fn init(config: &Config) {
    let level_filter = if let Ok(level) = env::var("RUST_LOG") {
        level
    } else {
        config.level_filter.to_string()
    };

    match config.format {
        Format::Compact => {
            tracing_subscriber::fmt()
                .compact()
                .with_target(false)
                .with_env_filter(EnvFilter::builder().parse_lossy(level_filter))
                .init();
        }
        Format::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_target(false)
                .flatten_event(true)
                .with_env_filter(EnvFilter::builder().parse_lossy(level_filter))
                .init();
        }
    }
}
