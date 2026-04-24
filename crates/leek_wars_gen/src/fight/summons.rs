//! Load `data/summons.json` (bulb templates) from the Java generator.

use crate::error::GenError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SummonTemplate {
    pub id: i32,
    pub name: String,
    pub chips: Vec<i32>,
    pub life: (i32, i32),
    pub strength: (i32, i32),
    pub wisdom: (i32, i32),
    pub agility: (i32, i32),
    pub resistance: (i32, i32),
    pub science: (i32, i32),
    pub magic: (i32, i32),
    pub tp: (i32, i32),
    pub mp: (i32, i32),
}

#[derive(Debug, Deserialize)]
struct SummonEntry {
    id: i32,
    name: String,
    #[serde(default)]
    chips: Vec<i32>,
    characteristics: Characteristics,
}

#[derive(Debug, Deserialize)]
struct Characteristics {
    life: [i32; 2],
    strength: [i32; 2],
    wisdom: [i32; 2],
    agility: [i32; 2],
    resistance: [i32; 2],
    science: [i32; 2],
    magic: [i32; 2],
    tp: [i32; 2],
    mp: [i32; 2],
}

pub fn load_summons_json(path: &Path) -> Result<HashMap<i32, SummonTemplate>, GenError> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let root: HashMap<String, SummonEntry> = serde_json::from_str(&raw)?;
    let mut by_id = HashMap::new();
    for (_k, s) in root {
        by_id.insert(
            s.id,
            SummonTemplate {
                id: s.id,
                name: s.name,
                chips: s.chips,
                life: (s.characteristics.life[0], s.characteristics.life[1]),
                strength: (s.characteristics.strength[0], s.characteristics.strength[1]),
                wisdom: (s.characteristics.wisdom[0], s.characteristics.wisdom[1]),
                agility: (s.characteristics.agility[0], s.characteristics.agility[1]),
                resistance: (
                    s.characteristics.resistance[0],
                    s.characteristics.resistance[1],
                ),
                science: (s.characteristics.science[0], s.characteristics.science[1]),
                magic: (s.characteristics.magic[0], s.characteristics.magic[1]),
                tp: (s.characteristics.tp[0], s.characteristics.tp[1]),
                mp: (s.characteristics.mp[0], s.characteristics.mp[1]),
            },
        );
    }
    Ok(by_id)
}
