/// Cato log levels.
///
/// Levels: quiet(0) → normal(1) → verbose(2) → debug(3)
/// Config: log_level = "normal" or log_level = "1"
/// Env override: CATO_LOG=verbose or CATO_LOG=2

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum LogLevel {
    Quiet = 0,
    Normal = 1,
    Verbose = 2,
    Debug = 3,
}

impl LogLevel {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "quiet" | "0" => LogLevel::Quiet,
            "normal" | "1" => LogLevel::Normal,
            "verbose" | "2" => LogLevel::Verbose,
            "debug" | "3" => LogLevel::Debug,
            _ => LogLevel::Normal,
        }
    }
}

static mut LOG_LEVEL: LogLevel = LogLevel::Normal;

/// Initialize the log level from env var or config.
/// Call once at startup. Env var takes precedence.
pub fn init(config_level: Option<&str>) {
    let level = if let Ok(env) = std::env::var("CATO_LOG") {
        LogLevel::from_str(&env)
    } else if let Ok(_) = std::env::var("CATO_DEBUG") {
        LogLevel::Debug
    } else if let Some(cfg) = config_level {
        LogLevel::from_str(cfg)
    } else {
        LogLevel::Normal
    };
    unsafe { LOG_LEVEL = level; }
}

/// Get current log level.
pub fn level() -> LogLevel {
    unsafe { LOG_LEVEL }
}

/// Print a message if the current log level is >= the given level.
#[macro_export]
macro_rules! cato_log {
    ($level:expr, $($arg:tt)*) => {
        if $crate::log::level() >= $level {
            eprintln!($($arg)*);
        }
    };
}
