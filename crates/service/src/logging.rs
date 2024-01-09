use crate::Config;

pub fn init<T>(config: &Config<T>) {
    env_logger::Builder::from_env(
        env_logger::Env::new().default_filter_or(config.log_level.as_deref().unwrap_or("info")),
    )
    .init();
}
