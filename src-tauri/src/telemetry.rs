use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;

static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static CAPTURED_LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

pub fn emit(event: &str, fields: &[(&str, String)]) {
    let line = format_line(event, fields);
    eprintln!("{line}");

    if CAPTURE_ENABLED.load(Ordering::Relaxed) {
        let buffer = CAPTURED_LINES.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut captured) = buffer.lock() {
            captured.push(line);
        }
    }
}

pub fn clear_captured_lines() {
    CAPTURE_ENABLED.store(true, Ordering::SeqCst);
    let buffer = CAPTURED_LINES.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut captured) = buffer.lock() {
        captured.clear();
    }
}

pub fn drain_captured_lines() -> Vec<String> {
    let buffer = CAPTURED_LINES.get_or_init(|| Mutex::new(Vec::new()));
    let lines = buffer
        .lock()
        .map(|mut captured| std::mem::take(&mut *captured))
        .unwrap_or_default();
    CAPTURE_ENABLED.store(false, Ordering::SeqCst);
    lines
}

fn format_line(event: &str, fields: &[(&str, String)]) -> String {
    let timestamp = Utc::now().to_rfc3339();
    let suffix = fields
        .iter()
        .map(|(key, value)| format!("{key}={}", sanitize_value(value)))
        .collect::<Vec<_>>()
        .join(" ");

    if suffix.is_empty() {
        format!("[backend] ts={timestamp} event={event}")
    } else {
        format!("[backend] ts={timestamp} event={event} {suffix}")
    }
}

fn sanitize_value(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            ' ' | '\n' | '\r' | '\t' => '_',
            _ => ch,
        })
        .collect()
}
