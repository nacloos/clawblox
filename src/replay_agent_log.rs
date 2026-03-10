use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Clone, Serialize)]
pub struct ReplayAgentMessage {
    pub id: String,
    pub role: String,
    pub timestamp_ms: i64,
    pub tick: f64,
    pub approx: bool,
    pub text: String,
}

#[derive(Clone)]
pub struct ReplayAgentTimeline {
    pub messages: Vec<ReplayAgentMessage>,
    pub anchor_count: usize,
}

#[derive(Clone)]
struct TickAnchor {
    ts_ms: i64,
    tick: f64,
}

struct RawReplayAgentMessage {
    id: String,
    role: String,
    timestamp_ms: i64,
    text: String,
}

pub fn load_agent_timeline(path: &Path, tick_rate: u32) -> Result<ReplayAgentTimeline, String> {
    let file = File::open(path).map_err(|e| format!("open failed: {}", e))?;
    let reader = BufReader::new(file);

    let mut anchors: Vec<TickAnchor> = Vec::new();
    let mut raw_messages: Vec<RawReplayAgentMessage> = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("read failed: {}", e))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()).unwrap_or_default() != "message" {
            continue;
        }

        let Some(message) = value.get("message") else {
            continue;
        };
        let role = message
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if role.is_empty() {
            continue;
        }
        let Some(ts_ms) = parse_agent_timestamp_ms(&value) else {
            continue;
        };
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if role == "toolResult" {
            if let Some(content) = message.get("content").and_then(|v| v.as_array()) {
                for item in content {
                    let text = item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if let Some(tick) = extract_tick_from_text(text) {
                        anchors.push(TickAnchor {
                            ts_ms,
                            tick: tick as f64,
                        });
                        break;
                    }
                }
            }
        }

        if role != "assistant" && role != "user" {
            continue;
        }
        let text = extract_replay_agent_message_text(message);
        if text.is_empty() {
            continue;
        }
        raw_messages.push(RawReplayAgentMessage {
            id,
            role,
            timestamp_ms: ts_ms,
            text,
        });
    }

    anchors.sort_by_key(|a| a.ts_ms);

    let tick_rate_per_ms = tick_rate as f64 / 1000.0;
    let mut messages = raw_messages
        .into_iter()
        .map(|raw| {
            let (tick, approx) =
                infer_tick_from_anchors(raw.timestamp_ms, &anchors, tick_rate_per_ms);
            ReplayAgentMessage {
                id: raw.id,
                role: raw.role,
                timestamp_ms: raw.timestamp_ms,
                tick,
                approx,
                text: raw.text,
            }
        })
        .collect::<Vec<_>>();

    messages.sort_by(|a, b| {
        a.tick
            .partial_cmp(&b.tick)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.timestamp_ms.cmp(&b.timestamp_ms))
    });

    Ok(ReplayAgentTimeline {
        messages,
        anchor_count: anchors.len(),
    })
}

fn parse_agent_timestamp_ms(value: &Value) -> Option<i64> {
    if let Some(ms) = value
        .get("message")
        .and_then(|m| m.get("timestamp"))
        .and_then(|v| v.as_i64())
    {
        return Some(ms);
    }
    let iso = value.get("timestamp").and_then(|v| v.as_str())?;
    let parsed = DateTime::parse_from_rfc3339(iso).ok()?;
    Some(parsed.timestamp_millis())
}

fn extract_replay_agent_message_text(message: &Value) -> String {
    let Some(content) = message.get("content").and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();

    for item in content {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        match item_type {
            "text" => {
                let text = item
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim();
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
            "thinking" => {
                let text = item
                    .get("thinking")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim();
                if !text.is_empty() {
                    parts.push(format!("thinking: {}", text));
                }
            }
            "toolCall" => {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                let command = item
                    .get("arguments")
                    .and_then(|v| v.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim();
                if command.is_empty() {
                    parts.push(format!("tool: {}", name));
                } else {
                    parts.push(format!("tool {}: {}", name, command));
                }
            }
            _ => {}
        }
    }

    let mut joined = parts.join("\n").trim().to_string();
    if joined.len() > 1200 {
        joined.truncate(1200);
        joined.push_str("...");
    }
    joined
}

fn extract_tick_from_text(text: &str) -> Option<u64> {
    let key_idx = text.find("\"tick\"")?;
    let after_key = &text[key_idx + "\"tick\"".len()..];
    let colon_idx = after_key.find(':')?;
    let mut saw_digit = false;
    let mut digits = String::new();
    for ch in after_key[colon_idx + 1..].chars() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            digits.push(ch);
            continue;
        }
        if saw_digit {
            break;
        }
    }
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

fn infer_tick_from_anchors(ts_ms: i64, anchors: &[TickAnchor], fallback_ticks_per_ms: f64) -> (f64, bool) {
    if anchors.is_empty() {
        return (0.0, true);
    }
    match anchors.binary_search_by_key(&ts_ms, |a| a.ts_ms) {
        Ok(i) => (anchors[i].tick.max(0.0), false),
        Err(0) => {
            let slope = if anchors.len() >= 2 {
                local_slope(&anchors[0], &anchors[1], fallback_ticks_per_ms)
            } else {
                fallback_ticks_per_ms
            };
            let dt = (ts_ms - anchors[0].ts_ms) as f64;
            ((anchors[0].tick + dt * slope).max(0.0), true)
        }
        Err(i) if i >= anchors.len() => {
            let last = anchors.len() - 1;
            let slope = if anchors.len() >= 2 {
                local_slope(&anchors[last - 1], &anchors[last], fallback_ticks_per_ms)
            } else {
                fallback_ticks_per_ms
            };
            let dt = (ts_ms - anchors[last].ts_ms) as f64;
            ((anchors[last].tick + dt * slope).max(0.0), true)
        }
        Err(i) => {
            let a = &anchors[i - 1];
            let b = &anchors[i];
            let dt = (b.ts_ms - a.ts_ms) as f64;
            if dt.abs() < f64::EPSILON {
                return (a.tick.max(0.0), true);
            }
            let t = (ts_ms - a.ts_ms) as f64 / dt;
            let tick = a.tick + (b.tick - a.tick) * t;
            (tick.max(0.0), true)
        }
    }
}

fn local_slope(a: &TickAnchor, b: &TickAnchor, fallback_ticks_per_ms: f64) -> f64 {
    let dt = (b.ts_ms - a.ts_ms) as f64;
    if dt.abs() < f64::EPSILON {
        fallback_ticks_per_ms
    } else {
        (b.tick - a.tick) / dt
    }
}
