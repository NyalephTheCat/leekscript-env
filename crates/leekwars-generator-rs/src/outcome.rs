use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub fight: Value,
    pub logs: Value,
    pub winner: i32,
    pub duration: i32,
    pub analyze_time: i64,
    pub compilation_time: i64,
    pub execution_time: i64,
}

impl Outcome {
    pub fn empty() -> Self {
        Self {
            fight: Value::Object(Default::default()),
            logs: Value::Object(Default::default()),
            winner: 0,
            duration: 0,
            analyze_time: 0,
            compilation_time: 0,
            execution_time: 0,
        }
    }

    pub fn snapshot_at(
        &self,
        action_index: usize,
    ) -> miette::Result<crate::snapshot::FightSnapshot> {
        crate::snapshot::snapshot_at_action_index(&self.fight, action_index)
    }
}
