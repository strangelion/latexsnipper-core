use log::{Level, LevelFilter, Log, Metadata, Record};
use std::sync::Mutex;

/// Simple logger that collects log messages for diagnostics.
pub struct CoreLogger {
    entries: Mutex<Vec<LogEntry>>,
    level: LevelFilter,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub timestamp: std::time::SystemTime,
}

impl CoreLogger {
    pub fn new(level: LevelFilter) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            level,
        }
    }

    pub fn get_entries(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl Log for CoreLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let entry = LogEntry {
                level: record.level(),
                target: record.target().to_string(),
                message: record.args().to_string(),
                timestamp: std::time::SystemTime::now(),
            };
            self.entries.lock().unwrap().push(entry);
        }
    }

    fn flush(&self) {}
}

/// Initialize the global logger. Call once at startup.
pub fn init_logger(level: LevelFilter) -> &'static CoreLogger {
    static LOGGER: once_cell::sync::Lazy<CoreLogger> =
        once_cell::sync::Lazy::new(|| CoreLogger::new(LevelFilter::Info));

    log::set_logger(&*LOGGER).ok();
    log::set_max_level(level);
    &LOGGER
}
