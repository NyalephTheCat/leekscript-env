#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum LaunchType {
    Line = 1,
    Diagonal = 2,
    Star = 3,
    StarInverted = 4,
    DiagonalInverted = 5,
    LineInverted = 6,
    Circle = 7,
}

impl LaunchType {
    #[must_use]
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Line,
            2 => Self::Diagonal,
            3 => Self::Star,
            4 => Self::StarInverted,
            5 => Self::DiagonalInverted,
            6 => Self::LineInverted,
            7 => Self::Circle,
            _ => return None,
        })
    }

    #[must_use]
    pub fn allows(self, dx: i32, dy: i32) -> bool {
        let dx = dx.abs();
        let dy = dy.abs();
        let is_line = dx == 0 || dy == 0;
        let is_diag = dx == dy;
        match self {
            Self::Line => is_line,
            Self::Diagonal => is_diag,
            Self::Star => is_line || is_diag,
            Self::StarInverted => !(is_line || is_diag),
            Self::DiagonalInverted => !is_diag,
            Self::LineInverted => !is_line,
            Self::Circle => true,
        }
    }

    #[must_use]
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum AreaId {
    Point = 1,
    LaserLine = 2,
    // Many more exist; we only special-case a few in logic.
    FirstInline = 13,
    Enemies = 14,
    Allies = 15,
}

impl AreaId {
    #[must_use]
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Point,
            2 => Self::LaserLine,
            13 => Self::FirstInline,
            14 => Self::Enemies,
            15 => Self::Allies,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum EffectType {
    Damage = 1,
    Heal = 2,
    BuffStrength = 3,
    BuffAgility = 4,
    RelativeShield = 5,
    AbsoluteShield = 6,
    Debuff = 9,
    Teleport = 10,
    Permutation = 11,
    Poison = 13,
    Summon = 14,
    Resurrect = 15,
    Kill = 16,
    ShackleMp = 17,
    ShackleTp = 18,
    ShackleStrength = 19,
    DamageReturn = 20,
    Antidote = 23,
    Vulnerability = 26,
    RemoveShackles = 49,
    Attract = 46,
    Push = 51,
    Repel = 53,
    AddState = 59,
    TotalDebuff = 60,
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
            9 => Self::Debuff,
            10 => Self::Teleport,
            11 => Self::Permutation,
            13 => Self::Poison,
            14 => Self::Summon,
            15 => Self::Resurrect,
            16 => Self::Kill,
            17 => Self::ShackleMp,
            18 => Self::ShackleTp,
            19 => Self::ShackleStrength,
            20 => Self::DamageReturn,
            23 => Self::Antidote,
            26 => Self::Vulnerability,
            49 => Self::RemoveShackles,
            46 => Self::Attract,
            51 => Self::Push,
            53 => Self::Repel,
            59 => Self::AddState,
            60 => Self::TotalDebuff,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ChipItemId(pub i64);

impl ChipItemId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ChipTemplateId(pub i64);

impl ChipTemplateId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct WeaponItemId(pub i64);

impl WeaponItemId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct WeaponTemplateId(pub i64);

impl WeaponTemplateId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SummonId(pub i64);

impl SummonId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct CellId(pub i64);

impl CellId {
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

