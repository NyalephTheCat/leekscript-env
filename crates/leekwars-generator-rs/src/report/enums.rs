#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum EffectType {
    Damage = 1,
    Heal = 2,
    BuffStrength = 3,
    BuffAgility = 4,
    RelativeShield = 5,
    AbsoluteShield = 6,
    BuffMp = 7,
    BuffTp = 8,
    Debuff = 9,
    Teleport = 10,
    Invert = 11,
    BoostMaxLife = 12,
    Poison = 13,
    Summon = 14,
    Resurrect = 15,
    Kill = 16,
    ShackleMp = 17,
    ShackleTp = 18,
    ShackleStrength = 19,
    DamageReturn = 20,
    Aftereffect = 25,
    Vulnerability = 26,
    LifeDamage = 28,
    NovaDamage = 30,
    NovaVitality = 45,
}

impl EffectType {
    #[must_use]
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Damage,
            2 => Self::Heal,
            3 => Self::BuffStrength,
            4 => Self::BuffAgility,
            5 => Self::RelativeShield,
            6 => Self::AbsoluteShield,
            7 => Self::BuffMp,
            8 => Self::BuffTp,
            9 => Self::Debuff,
            10 => Self::Teleport,
            11 => Self::Invert,
            12 => Self::BoostMaxLife,
            13 => Self::Poison,
            14 => Self::Summon,
            15 => Self::Resurrect,
            16 => Self::Kill,
            17 => Self::ShackleMp,
            18 => Self::ShackleTp,
            19 => Self::ShackleStrength,
            20 => Self::DamageReturn,
            25 => Self::Aftereffect,
            26 => Self::Vulnerability,
            28 => Self::LifeDamage,
            30 => Self::NovaDamage,
            45 => Self::NovaVitality,
            _ => return None,
        })
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Damage => "DAMAGE",
            Self::Heal => "HEAL",
            Self::BuffStrength => "BUFF_STRENGTH",
            Self::BuffAgility => "BUFF_AGILITY",
            Self::RelativeShield => "RELATIVE_SHIELD",
            Self::AbsoluteShield => "ABSOLUTE_SHIELD",
            Self::BuffMp => "BUFF_MP",
            Self::BuffTp => "BUFF_TP",
            Self::Debuff => "DEBUFF",
            Self::Teleport => "TELEPORT",
            Self::Invert => "INVERT",
            Self::BoostMaxLife => "BOOST_MAX_LIFE",
            Self::Poison => "POISON",
            Self::Summon => "SUMMON",
            Self::Resurrect => "RESURRECT",
            Self::Kill => "KILL",
            Self::ShackleMp => "SHACKLE_MP",
            Self::ShackleTp => "SHACKLE_TP",
            Self::ShackleStrength => "SHACKLE_STRENGTH",
            Self::DamageReturn => "DAMAGE_RETURN",
            Self::Aftereffect => "AFTEREFFECT",
            Self::Vulnerability => "VULNERABILITY",
            Self::LifeDamage => "LIFE_DAMAGE",
            Self::NovaDamage => "NOVA_DAMAGE",
            Self::NovaVitality => "NOVA_VITALITY",
        }
    }
}
