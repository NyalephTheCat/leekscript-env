use crate::error::GenError;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RunOutput {
    pub outcome_json: String,
}

#[derive(Debug, Clone, Default)]
pub struct SimOptions {
    pub only_code: Option<i64>,
    pub only_fid: Option<i64>,
    pub limit: Option<usize>,
    pub diff_friendly: bool,
    pub group_turns: bool,
    pub style: SimStyle,
    pub show_indices: bool,
    pub verbosity: SimVerbosity,
    /// Optional fid→display name mapping (when outcome JSON does not carry names).
    pub fid_names: Option<std::collections::HashMap<i64, String>>,
    /// Optional scenario data lookup tables (chips/weapons) for nicer formatting.
    pub data: Option<SimData>,
    /// When true, include `(hp=..., Δhp=...)` on stat-change lines when available.
    pub show_live_stats: bool,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum SimVerbosity {
    /// Show only high-signal events (damage/heal/deaths/chip+weapon use, summons).
    Brief,
    #[default]
    Normal,
    /// Include extra low-signal events; may include fallback JSON for unknown actions.
    Verbose,
}

#[derive(Debug, Clone, Default)]
pub struct SimData {
    /// `chipTemplateId` (from actions) -> chip name.
    pub chip_name_by_template: std::collections::HashMap<i64, String>,
    /// `weaponTemplateId` (from `set_weapon` actions / UI) -> weapon name.
    pub weapon_name_by_template: std::collections::HashMap<i64, String>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum SimStyle {
    /// LW-inspired human log (turn headers + entity blocks + short lines).
    #[default]
    Pretty,
    /// Raw per-action dump (mostly for debugging).
    Raw,
}

pub fn parse_outcome(outcome_json: &str) -> Result<Value, GenError> {
    serde_json::from_str(outcome_json)
        .map_err(|e| GenError::Message(format!("outcome is not JSON: {e}")))
}

#[must_use]
pub fn pretty_summary(outcome: &Value) -> String {
    let winner = outcome.get("winner").cloned().unwrap_or(Value::Null);
    let duration = outcome.get("duration").cloned().unwrap_or(Value::Null);
    let ops = outcome
        .get("fight")
        .and_then(|f| f.get("ops"))
        .and_then(|v| v.as_array())
        .map(std::vec::Vec::len);
    let actions = outcome
        .get("fight")
        .and_then(|f| f.get("actions"))
        .and_then(|v| v.as_array())
        .map(std::vec::Vec::len);

    let mut s = String::new();
    s.push_str("Outcome\n");
    s.push_str(&format!("  winner:   {winner}\n"));
    s.push_str(&format!("  duration: {duration}\n"));
    if let Some(n) = actions {
        s.push_str(&format!("  actions:  {n}\n"));
    }
    if let Some(n) = ops {
        s.push_str(&format!("  ops:      {n}\n"));
    }
    s
}

#[must_use]
pub fn sim_text(outcome: &Value) -> String {
    sim_text_with_options(outcome, &SimOptions::default())
}

#[must_use]
pub fn sim_text_with_options(outcome: &Value, opts: &SimOptions) -> String {
    let mut out = String::new();
    out.push_str("Simulation\n");
    let actions = outcome
        .get("fight")
        .and_then(|f| f.get("actions"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let fid_names = opts
        .fid_names
        .clone()
        .unwrap_or_else(|| fid_name_map_from_outcome(outcome));
    let data = opts.data.clone().unwrap_or_default();
    let mut current_turn: i64 = 1;
    let mut active_fid: Option<i64> = None;
    let mut weapon_by_fid: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    let mut hp_by_fid: std::collections::HashMap<i64, i64> = initial_hp_map(outcome);
    let mut turn_start_hp_by_fid: std::collections::HashMap<i64, i64> =
        std::collections::HashMap::new();
    let mut emitted: usize = 0;

    for (i, a) in actions.iter().enumerate() {
        let code = a
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);

        // Best-effort turn tracking from known official-generator codes emitted by our Rust engine.
        // [6, turn] = ActionNewTurn, [7, fid] = ActionEntityTurn, [8, fid, tp, mp] = ActionEndTurn
        if code == 6 {
            if let Some(t) = a
                .as_array()
                .and_then(|arr| arr.get(1))
                .and_then(serde_json::Value::as_i64)
            {
                current_turn = t.max(1);
            }
            active_fid = None;
        } else if code == 7 {
            active_fid = a
                .as_array()
                .and_then(|arr| arr.get(1))
                .and_then(serde_json::Value::as_i64);
            if let Some(fid) = active_fid {
                if let Some(hp) = hp_by_fid.get(&fid).copied() {
                    turn_start_hp_by_fid.insert(fid, hp);
                }
            }
        } else if code == 8 {
            // Keep fid visible but we can still attribute the action to its fid.
            active_fid = a
                .as_array()
                .and_then(|arr| arr.get(1))
                .and_then(serde_json::Value::as_i64);
        } else if code == 13 {
            // [SET_WEAPON, weaponTemplate]
            // Attribute to active fid (set by [7,fid]) when possible.
            if let Some(fid) = active_fid {
                if let Some(wt) = a
                    .as_array()
                    .and_then(|arr| arr.get(1))
                    .and_then(serde_json::Value::as_i64)
                {
                    weapon_by_fid.insert(fid, wt);
                }
            }
        }

        // Track HP deltas from actions (best-effort).
        // [101, target_fid, dmg, erosion] and [103, target_fid, heal]
        if code == 101 {
            if let Some(arr) = a.as_array() {
                let tfid = arr.get(1).and_then(serde_json::Value::as_i64);
                let dmg = arr
                    .get(2)
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                if let Some(fid) = tfid {
                    let cur = hp_by_fid.get(&fid).copied().unwrap_or(0);
                    hp_by_fid.insert(fid, (cur - dmg).max(0));
                }
            }
        } else if code == 103 {
            if let Some(arr) = a.as_array() {
                let tfid = arr.get(1).and_then(serde_json::Value::as_i64);
                let heal = arr
                    .get(2)
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                if let Some(fid) = tfid {
                    let cur = hp_by_fid.get(&fid).copied().unwrap_or(0);
                    hp_by_fid.insert(fid, cur + heal);
                }
            }
        }

        if let Some(only) = opts.only_code {
            if code != only {
                continue;
            }
        }
        if let Some(only) = opts.only_fid {
            // Include if:
            // - the action's second element is the fid, or
            // - the currently active fid (from [7, fid]) matches.
            let second = a
                .as_array()
                .and_then(|arr| arr.get(1))
                .and_then(serde_json::Value::as_i64);
            if second != Some(only) && active_fid != Some(only) {
                continue;
            }
        }

        if !verbosity_keeps(opts.verbosity, code) {
            continue;
        }

        match opts.style {
            SimStyle::Raw => {
                if opts.group_turns && (emitted == 0 || code == 6) {
                    out.push_str(&format!("\n-- turn {current_turn} --\n"));
                }

                if opts.diff_friendly {
                    // Stable, line-oriented: omit full JSON, keep a short preview.
                    let fid_label = active_fid.map(|f| format!(" fid={f}")).unwrap_or_default();
                    out.push_str(&format!(
                        "  [{i:04}] turn={current_turn}{fid_label} code={code}  {}\n",
                        preview_json(a, 96)
                    ));
                } else {
                    out.push_str(&format!(
                        "  [{i:04}] code={code}  {}\n",
                        preview_json(a, 160)
                    ));
                }
            }
            SimStyle::Pretty => {
                // Pretty mode always groups by turns and prints entity turn headers.
                if code == 6 {
                    out.push_str(&format!("\nTurn {current_turn}\n"));
                    continue;
                }
                if emitted == 0 {
                    out.push_str(&format!("\nTurn {current_turn}\n"));
                }
                if code == 7 {
                    let fid = active_fid.unwrap_or(-1);
                    let name = fid_names
                        .get(&fid)
                        .cloned()
                        .unwrap_or_else(|| format!("Leek #{fid}"));
                    out.push_str(&format!("  {name}\n"));
                    continue;
                }

                // Action line (indented under the current entity, when known).
                let prefix = if opts.show_indices {
                    format!("    [{i:04}] ")
                } else {
                    "    - ".to_string()
                };
                let include_actor =
                    action_primary_fid(a).is_some_and(|fid| Some(fid) != active_fid);
                let line = format_action_pretty_with_state(
                    code,
                    a,
                    active_fid,
                    &fid_names,
                    include_actor,
                    &PrettyActionState {
                        data: &data,
                        weapon_by_fid: &weapon_by_fid,
                        hp_by_fid: &hp_by_fid,
                        turn_start_hp_by_fid: &turn_start_hp_by_fid,
                        show_live_stats: opts.show_live_stats,
                    },
                );
                out.push_str(&format!("{prefix}{line}\n"));
            }
        }

        emitted += 1;
        if let Some(limit) = opts.limit {
            if emitted >= limit {
                out.push_str("  … (limit reached)\n");
                break;
            }
        }
    }
    out
}

fn verbosity_keeps(v: SimVerbosity, code: i64) -> bool {
    match v {
        SimVerbosity::Verbose | SimVerbosity::Normal => true,
        SimVerbosity::Brief => matches!(
            code,
            5 | 11 | 12 | 16 | 9 | 105 | 101 | 103 | 104 | 107 | 108 | 110 | 111 | 1002
        ),
    }
}

fn action_primary_fid(action: &Value) -> Option<i64> {
    action
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(serde_json::Value::as_i64)
}

fn initial_hp_map(outcome: &Value) -> std::collections::HashMap<i64, i64> {
    let mut out = std::collections::HashMap::new();
    let leeks = outcome
        .get("fight")
        .and_then(|f| f.get("leeks"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for l in leeks {
        let Some(obj) = l.as_object() else { continue };
        let Some(id) = obj.get("id").and_then(serde_json::Value::as_i64) else {
            continue;
        };
        let life = obj
            .get("life")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        out.insert(id, life);
    }
    out
}

fn fid_name_map_from_outcome(outcome: &Value) -> std::collections::HashMap<i64, String> {
    let mut out = std::collections::HashMap::new();
    let leeks = outcome
        .get("fight")
        .and_then(|f| f.get("leeks"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for l in leeks {
        let Some(obj) = l.as_object() else { continue };
        let Some(id) = obj.get("id").and_then(serde_json::Value::as_i64) else {
            continue;
        };
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Leek")
            .to_string();
        out.insert(id, format!("{name} #{id}"));
    }
    out
}

struct PrettyActionState<'a> {
    data: &'a SimData,
    weapon_by_fid: &'a std::collections::HashMap<i64, i64>,
    hp_by_fid: &'a std::collections::HashMap<i64, i64>,
    turn_start_hp_by_fid: &'a std::collections::HashMap<i64, i64>,
    show_live_stats: bool,
}

fn format_action_pretty_with_state(
    code: i64,
    action: &Value,
    active_fid: Option<i64>,
    fid_names: &std::collections::HashMap<i64, String>,
    include_actor: bool,
    st: &PrettyActionState<'_>,
) -> String {
    let PrettyActionState {
        data,
        weapon_by_fid,
        hp_by_fid,
        turn_start_hp_by_fid,
        show_live_stats: show_ls,
    } = st;
    let show_live_stats = *show_ls;
    match code {
        101 => {
            // LOST_LIFE: [101, target_fid, dmg, erosion_or_absorbed]
            let arr = action.as_array().cloned().unwrap_or_default();
            let fid = arr
                .get(1)
                .and_then(serde_json::Value::as_i64)
                .or(active_fid)
                .unwrap_or(-1);
            let dmg = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0);
            let erosion = arr.get(3).and_then(serde_json::Value::as_i64).unwrap_or(0);
            let who = fid_names
                .get(&fid)
                .cloned()
                .unwrap_or_else(|| format!("Leek #{fid}"));
            let hp = hp_by_fid.get(&fid).copied();
            let start = turn_start_hp_by_fid.get(&fid).copied();
            let d_turn = match (hp, start) {
                (Some(h), Some(s)) => Some(h - s),
                _ => None,
            };
            let stats = if show_live_stats {
                match (hp, d_turn) {
                    (Some(h), Some(dt)) => format!(" (hp={h}, Δhp={dt})"),
                    (Some(h), None) => format!(" (hp={h})"),
                    _ => String::new(),
                }
            } else {
                String::new()
            };
            if include_actor {
                format!("{who} loses {dmg} hp (erosion={erosion}){stats}")
            } else {
                format!("loses {dmg} hp (erosion={erosion}){stats}")
            }
        }
        103 => {
            // HEAL: [103, target_fid, heal]
            let arr = action.as_array().cloned().unwrap_or_default();
            let fid = arr
                .get(1)
                .and_then(serde_json::Value::as_i64)
                .or(active_fid)
                .unwrap_or(-1);
            let heal = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0);
            let who = fid_names
                .get(&fid)
                .cloned()
                .unwrap_or_else(|| format!("Leek #{fid}"));
            let hp = hp_by_fid.get(&fid).copied();
            let start = turn_start_hp_by_fid.get(&fid).copied();
            let d_turn = match (hp, start) {
                (Some(h), Some(s)) => Some(h - s),
                _ => None,
            };
            let stats = if show_live_stats {
                match (hp, d_turn) {
                    (Some(h), Some(dt)) => format!(" (hp={h}, Δhp={dt})"),
                    (Some(h), None) => format!(" (hp={h})"),
                    _ => String::new(),
                }
            } else {
                String::new()
            };
            if include_actor {
                format!("{who} heals {heal}{stats}")
            } else {
                format!("heals {heal}{stats}")
            }
        }
        104 => {
            // VITALITY: [104, target_fid, add]
            let arr = action.as_array().cloned().unwrap_or_default();
            let fid = arr
                .get(1)
                .and_then(serde_json::Value::as_i64)
                .or(active_fid)
                .unwrap_or(-1);
            let add = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0);
            let who = fid_names
                .get(&fid)
                .cloned()
                .unwrap_or_else(|| format!("Leek #{fid}"));
            let hp = hp_by_fid.get(&fid).copied();
            let stats = if show_live_stats {
                hp.map(|h| format!(" (hp={h})")).unwrap_or_default()
            } else {
                String::new()
            };
            if include_actor {
                format!("{who} gains vitality {add}{stats}")
            } else {
                format!("gains vitality {add}{stats}")
            }
        }
        12 => {
            let arr = action.as_array().cloned().unwrap_or_default();
            let tpl = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let cell = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let res = arr.get(3).map(|v| preview_json(v, 90)).unwrap_or_default();
            let chip_name = data
                .chip_name_by_template
                .get(&tpl)
                .cloned()
                .unwrap_or_else(|| format!("chip#{tpl}"));
            if include_actor {
                let who = active_fid
                    .and_then(|f| fid_names.get(&f).cloned())
                    .unwrap_or_else(|| active_fid.map_or("?".into(), |f| format!("#{f}")));
                format!("{who} casts {chip_name} on cell {cell} -> {res}")
            } else {
                format!("casts {chip_name} on cell {cell} -> {res}")
            }
        }
        13 => {
            let arr = action.as_array().cloned().unwrap_or_default();
            let tpl = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let weapon_name = data
                .weapon_name_by_template
                .get(&tpl)
                .cloned()
                .unwrap_or_else(|| format!("weapon#{tpl}"));
            if include_actor {
                let who = active_fid
                    .and_then(|f| fid_names.get(&f).cloned())
                    .unwrap_or_else(|| active_fid.map_or("?".into(), |f| format!("#{f}")));
                format!("{who} equips {weapon_name}")
            } else {
                format!("equips {weapon_name}")
            }
        }
        16 => {
            let arr = action.as_array().cloned().unwrap_or_default();
            let cell = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let res = arr.get(2).map(|v| preview_json(v, 100)).unwrap_or_default();
            let weapon_tpl = active_fid.and_then(|fid| weapon_by_fid.get(&fid).copied());
            let weapon_name = weapon_tpl
                .and_then(|tpl| data.weapon_name_by_template.get(&tpl).cloned())
                .or_else(|| weapon_tpl.map(|tpl| format!("weapon#{tpl}")))
                .unwrap_or_else(|| "weapon".into());
            if include_actor {
                let who = active_fid
                    .and_then(|f| fid_names.get(&f).cloned())
                    .unwrap_or_else(|| active_fid.map_or("?".into(), |f| format!("#{f}")));
                format!("{who} uses {weapon_name} at cell {cell} -> {res}")
            } else {
                format!("uses {weapon_name} at cell {cell} -> {res}")
            }
        }
        8 => {
            // END_TURN: [8, fid, tp, mp]
            let arr = action.as_array().cloned().unwrap_or_default();
            let fid = arr
                .get(1)
                .and_then(serde_json::Value::as_i64)
                .or(active_fid)
                .unwrap_or(-1);
            let tp = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let mp = arr.get(3).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let hp = hp_by_fid.get(&fid).copied();
            let start = turn_start_hp_by_fid.get(&fid).copied();
            let delta = match (hp, start) {
                (Some(h), Some(s)) => Some(h - s),
                _ => None,
            };
            if include_actor {
                let who = fid_names
                    .get(&fid)
                    .cloned()
                    .unwrap_or_else(|| format!("Leek #{fid}"));
                if let (Some(hp), Some(d)) = (hp, delta) {
                    format!("{who} turn ends (tp={tp}, mp={mp}, hp={hp}, Δhp={d})")
                } else {
                    format!("{who} turn ends (tp={tp}, mp={mp})")
                }
            } else if let (Some(hp), Some(d)) = (hp, delta) {
                format!("turn ends (tp={tp}, mp={mp}, hp={hp}, Δhp={d})")
            } else {
                format!("turn ends (tp={tp}, mp={mp})")
            }
        }
        14 => {
            // STACK_EFFECT: [14, log_id, addedValue]
            let arr = action.as_array().cloned().unwrap_or_default();
            let log_id = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let add = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0);
            format!("stack effect (log_id={log_id}, +{add})")
        }
        303 => {
            let arr = action.as_array().cloned().unwrap_or_default();
            let log_id = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            format!("remove effect (log_id={log_id})")
        }
        _ => format_action_pretty(code, action, active_fid, fid_names, include_actor),
    }
}

fn format_action_pretty(
    code: i64,
    action: &Value,
    active_fid: Option<i64>,
    fid_names: &std::collections::HashMap<i64, String>,
    include_actor: bool,
) -> String {
    let arr = action.as_array().cloned().unwrap_or_default();
    let fid = arr
        .get(1)
        .and_then(serde_json::Value::as_i64)
        .or(active_fid);
    let who = fid
        .and_then(|f| fid_names.get(&f).cloned())
        .unwrap_or_else(|| fid.map_or_else(|| "?".into(), |f| format!("#{f}")));

    let who_prefix = if include_actor {
        format!("{who} ")
    } else {
        String::new()
    };

    match code {
        0 => "fight starts".into(),
        5 => format!(
            "{}dies (killed_by={})",
            who_prefix,
            arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(-1)
        ),
        6 => format!(
            "turn {}",
            arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1)
        ),
        7 => format!("{who_prefix}turn starts"),
        8 => format!(
            "{}turn ends (tp={}, mp={})",
            who_prefix,
            arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(-1),
            arr.get(3).and_then(serde_json::Value::as_i64).unwrap_or(-1)
        ),
        9 => format!("summon {:?}", preview_json(action, 120)),
        10 => {
            let dest = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let steps = arr
                .get(3)
                .and_then(|v| v.as_array())
                .map_or(0, |p| p.len().saturating_sub(1));
            format!("{who_prefix}moves to cell {dest} ({steps} steps)")
        }
        11 => format!("{who} is killed"),
        12 => {
            let chip = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let cell = arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let res = arr.get(3).map(|v| preview_json(v, 90)).unwrap_or_default();
            format!("use chip {chip} on cell {cell} -> {res}")
        }
        13 => format!(
            "{}sets weapon {}",
            who_prefix,
            arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1)
        ),
        14 => format!(
            "stack effect log_id={} +{}",
            arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1),
            arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0)
        ),
        16 => {
            let cell = arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let res = arr.get(2).map(|v| preview_json(v, 100)).unwrap_or_default();
            format!("{who_prefix}uses weapon at cell {cell} -> {res}")
        }
        101 => format!(
            "{}loses {} hp (erosion={})",
            who_prefix,
            arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0),
            arr.get(3).and_then(serde_json::Value::as_i64).unwrap_or(0)
        ),
        103 => format!(
            "{}heals {}",
            who_prefix,
            arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0)
        ),
        104 => format!(
            "{}gains vitality {}",
            who_prefix,
            arr.get(2).and_then(serde_json::Value::as_i64).unwrap_or(0)
        ),
        105 => format!("{who_prefix}resurrects"),
        107 => format!("{who} nova damage {:?}", preview_json(action, 100)),
        108 => format!("{who} damage return {:?}", preview_json(action, 100)),
        110 => format!("{who} poison damage {:?}", preview_json(action, 100)),
        111 => format!("{who} aftereffect {:?}", preview_json(action, 100)),
        203 => {
            let msg = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
            format!("{who_prefix}says: {msg:?}")
        }
        303 => format!(
            "remove effect log_id={}",
            arr.get(1).and_then(serde_json::Value::as_i64).unwrap_or(-1)
        ),
        306 => format!("{who} reduces effects {:?}", preview_json(action, 100)),
        307 => format!("{who} removes poisons"),
        308 => format!("{who} removes shackles"),
        1002 => format!("{who} AI error"),
        _ => {
            let name = action_code_name(code).unwrap_or("action");
            format!("{name} {}", preview_json(action, 140))
        }
    }
}

fn action_code_name(code: i64) -> Option<&'static str> {
    Some(match code {
        0 => "start_fight",
        5 => "player_dead",
        6 => "new_turn",
        7 => "entity_turn",
        8 => "end_turn",
        9 => "summon",
        10 => "move_to",
        11 => "kill",
        12 => "use_chip",
        13 => "set_weapon",
        14 => "stack_effect",
        16 => "use_weapon",
        101 => "lost_life",
        103 => "heal",
        104 => "vitality",
        105 => "resurrect",
        107 => "nova_damage",
        108 => "damage_return",
        110 => "poison_damage",
        111 => "aftereffect",
        203 => "say",
        303 => "remove_launched_effect",
        306 => "reduce_effects",
        307 => "remove_poisons",
        308 => "remove_shackles",
        1002 => "ai_error",
        _ => return None,
    })
}

fn preview_json(v: &Value, max: usize) -> String {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "<unprintable>".into());
    if s.len() <= max {
        s
    } else {
        format!("{}…", &s[..max])
    }
}
