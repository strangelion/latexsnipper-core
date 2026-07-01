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
    pub file: Option<String>,
    pub line: Option<u32>,
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

    pub fn get_entries_by_level(&self, level: Level) -> Vec<LogEntry> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.level == level)
            .cloned()
            .collect()
    }

    pub fn get_errors(&self) -> Vec<LogEntry> {
        self.get_entries_by_level(Level::Error)
    }

    pub fn get_warnings(&self) -> Vec<LogEntry> {
        self.get_entries_by_level(Level::Warn)
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
                file: record.file().map(|s| s.to_string()),
                line: record.line(),
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
