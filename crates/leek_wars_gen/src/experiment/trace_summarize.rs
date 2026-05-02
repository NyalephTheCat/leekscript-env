//! Summarize `trace.jsonl` from experiments into compact feature JSON.

use crate::error::GenError;
use crate::fight::TraceEvent;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct TraceSummary {
    pub n_events: usize,
    pub by_kind: HashMap<String, usize>,
    pub end_entity_turn_samples: usize,
    pub min_life: Option<i64>,
    pub max_life: Option<i64>,
}

/// Parse trace file: first line may be schema header JSON; rest are [`TraceEvent`] lines.
pub fn load_trace_jsonl(path: &Path) -> Result<Vec<TraceEvent>, GenError> {
    let raw = std::fs::read_to_string(path).map_err(GenError::from)?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).map_err(|e| GenError::Message(e.to_string()))?;
        if v.get("schema").is_some() {
            continue;
        }
        let ev: TraceEvent = serde_json::from_value(v).map_err(|e| GenError::Message(e.to_string()))?;
        out.push(ev);
    }
    Ok(out)
}

pub fn summarize_trace_events(events: &[TraceEvent]) -> TraceSummary {
    let mut s = TraceSummary {
        n_events: events.len(),
        ..Default::default()
    };
    for e in events {
        *s.by_kind.entry(e.kind.clone()).or_insert(0) += 1;
        if e.kind == "end_entity_turn" {
            s.end_entity_turn_samples += 1;
            if let Some(d) = e.detail.as_ref() {
                if let Some(l) = d.get("life").and_then(|x| x.as_i64()) {
                    s.min_life = Some(s.min_life.map_or(l, |m| m.min(l)));
                    s.max_life = Some(s.max_life.map_or(l, |m| m.max(l)));
                }
            }
        }
    }
    s
}

pub fn summarize_trace_file(path: &Path) -> Result<TraceSummary, GenError> {
    let ev = load_trace_jsonl(path)?;
    Ok(summarize_trace_events(&ev))
}
