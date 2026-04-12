//! Human-readable summaries for JSON API payloads (non-`--json` mode).

use serde_json::Value;

pub fn print_garden_summary(garden: &Value, solo: bool) -> anyhow::Result<()> {
    eprintln!(
        "--- garden ({}) ---",
        if solo { "solo context" } else { "farmer" }
    );
    if let Some(q) = garden.get("queue") {
        eprintln!("queue: {}", serde_json::to_string(q)?);
    }
    Ok(())
}

pub fn print_opponents_table(opponents: &Value) -> anyhow::Result<()> {
    eprintln!("--- opponents ---");
    let rows = opponents
        .as_array()
        .or_else(|| opponents.get("opponents").and_then(|x| x.as_array()))
        .or_else(|| opponents.get("leeks").and_then(|x| x.as_array()))
        .or_else(|| opponents.get("farmers").and_then(|x| x.as_array()));
    let Some(rows) = rows else {
        println!("{}", serde_json::to_string_pretty(opponents)?);
        return Ok(());
    };
    println!("{:>6}  {:>5}  {:>6}  {}", "id", "level", "talent", "name");
    for o in rows {
        let id = o.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let level = o.get("level").and_then(|x| x.as_i64()).unwrap_or(0);
        let talent = o.get("talent").and_then(|x| x.as_i64()).unwrap_or(0);
        let name = o.get("name").and_then(|x| x.as_str()).unwrap_or("?");
        println!("{id:>6}  {level:>5}  {talent:>6}  {name}");
    }
    Ok(())
}

pub fn print_leek_summary(v: &Value) {
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("?");
    let level = v.get("level").and_then(|x| x.as_i64()).unwrap_or(0);
    let talent = v.get("talent").and_then(|x| x.as_i64()).unwrap_or(0);
    println!("Leek {} — level {} — talent {}", name, level, talent);
}

pub fn print_leek_equipment(v: &Value) -> anyhow::Result<()> {
    println!("Weapons:");
    if let Some(arr) = v.get("weapons").and_then(|x| x.as_array()) {
        for w in arr {
            let id = w.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            let tpl = w.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            println!("  instance {id}  template {tpl}");
        }
    }
    println!("Chips:");
    if let Some(arr) = v.get("chips").and_then(|x| x.as_array()) {
        for w in arr {
            let id = w.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            let tpl = w.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            println!("  instance {id}  template {tpl}");
        }
    }
    if let Some(h) = v.get("hat").and_then(|x| x.as_object()) {
        println!("Hat: {:?}", h.get("name").or_else(|| h.get("id")));
    }
    Ok(())
}

pub fn print_farmer_inventory(farmer: &Value) -> anyhow::Result<()> {
    let print_items = |label: &str, key: &str| -> anyhow::Result<()> {
        println!("{label}:");
        if let Some(arr) = farmer.get(key).and_then(|x| x.as_array()) {
            for it in arr {
                let id = it.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
                let tpl = it.get("template").and_then(|x| x.as_i64());
                let name = it.get("name").and_then(|x| x.as_str());
                let qty = it.get("quantity").and_then(|x| x.as_i64()).unwrap_or(1);
                match (tpl, name) {
                    (Some(t), Some(n)) => println!("  id {id}  template {t}  x{qty}  {n}"),
                    (Some(t), None) => println!("  id {id}  template {t}  x{qty}"),
                    _ => println!("  {}", serde_json::to_string(it)?),
                }
            }
        }
        Ok(())
    };
    print_items("Weapons (inventory)", "weapons")?;
    print_items("Chips (inventory)", "chips")?;
    print_items("Potions", "potions")?;
    print_items("Hats", "hats")?;
    Ok(())
}
