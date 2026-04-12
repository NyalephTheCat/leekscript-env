use super::types::{
    ChipItemId, ChipTemplateId, EffectType, LaunchType, SummonId, WeaponItemId, WeaponTemplateId,
};

#[derive(Debug, Clone)]
pub struct ChipEffectDef {
    pub id: EffectType,
    pub value1: f64,
    pub value2: f64,
    pub turns: i64,
    pub targets: i64,
    pub modifiers: i64,
    pub r#type: i64,
}

#[derive(Debug, Clone)]
pub struct ChipDef {
    pub item: ChipItemId,
    pub template: ChipTemplateId,
    pub cost: i64,
    pub min_range: i64,
    pub max_range: i64,
    pub launch_type: LaunchType,
    pub area: i64,
    pub los: bool,
    pub cooldown: i64,
    pub team_cooldown: bool,
    pub initial_cooldown: i64,
    pub max_uses: i64,
    pub effects: Vec<ChipEffectDef>,
}

#[derive(Debug, Clone)]
pub struct SummonDef {
    pub id: SummonId,
    pub name: String,
    pub chips: Vec<i64>,
    pub life_range: (i64, i64),
    pub tp_range: (i64, i64),
    pub mp_range: (i64, i64),
    pub strength_range: (i64, i64),
}

#[derive(Debug, Clone)]
pub struct EffectInstance {
    pub instance_id: i64,
    pub item_id: i64,
    pub caster: i64,
    pub target: i64,
    pub effect_id: EffectType,
    pub value: i64,
    pub turns_left: i64,
    pub modifiers: i64,
    pub from_weapon: bool,
}

#[derive(Debug, Clone)]
pub struct WeaponDef {
    pub item: WeaponItemId,
    pub template: WeaponTemplateId,
    pub cost: i64,
    pub min_range: i64,
    pub max_range: i64,
    pub launch_type: LaunchType,
    pub base_damage: i64,
    pub los: bool,
    pub area: i64,
    pub max_uses: i64,
}
