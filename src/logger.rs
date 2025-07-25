//! Logging implementation for mdevctl

pub fn logger() -> env_logger::Builder {
    let env = env_logger::Env::new()
        .filter_or("MDEVCTL_LOG", "warn")
        .write_style("MDEVCTL_LOG_STYLE");
    let mut b = env_logger::Builder::from_env(env);
    b.format_source_path(true);
    b
}
