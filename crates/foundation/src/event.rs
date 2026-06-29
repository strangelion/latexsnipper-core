use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Event types emitted by the Core.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    /// Model loading progress (0.0 - 1.0).
    ModelLoadProgress,
    /// Recognition started.
    RecognitionStarted,
    /// Recognition completed.
    RecognitionCompleted,
    /// Recognition failed.
    RecognitionFailed,
    /// Export started.
    ExportStarted,
    /// Export completed.
    ExportCompleted,
    /// Pipeline node executed.
    PipelineNodeExecuted,
    /// Custom event.
    Custom(String),
}

/// An event payload.
#[derive(Debug, Clone)]
pub struct Event {
    pub event_type: EventType,
    pub data: serde_json::Value,
}

/// Event listener callback type.
pub type EventListener = Arc<dyn Fn(&Event) + Send + Sync>;

/// Central event bus for Core events.
pub struct EventBus {
    listeners: RwLock<HashMap<EventType, Vec<EventListener>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            listeners: RwLock::new(HashMap::new()),
        }
    }

    pub fn subscribe(&self, event_type: EventType, listener: EventListener) {
        self.listeners
            .write()
            .unwrap()
            .entry(event_type)
            .or_default()
            .push(listener);
    }

    pub fn emit(&self, event: Event) {
        if let Some(listeners) = self.listeners.read().unwrap().get(&event.event_type) {
            for listener in listeners {
                listener(&event);
            }
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
