//! Leek creation, garden, equipment, AI binding, capital.

use serde::Serialize;
use serde_json::{Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

#[derive(Debug, Serialize)]
pub struct LeekCreateRequest<'a> {
    pub name: &'a str,
}

#[derive(Debug, Serialize)]
pub struct LeekIdBody {
    pub leek_id: i64,
}

#[derive(Debug, Serialize)]
pub struct LeekSetInGardenBody {
    pub leek_id: i64,
    pub in_garden: bool,
}

#[derive(Debug, Serialize)]
pub struct LeekSetAiBody {
    pub leek_id: i64,
    pub ai_id: i64,
}

#[derive(Debug, Serialize)]
pub struct LeekSpendCapitalBody<'a> {
    pub leek_id: i64,
    /// JSON object of characteristic bonuses (same as web: `JSON.stringify(this.bonuses)`).
    pub characteristics: &'a str,
}

#[derive(Debug, Serialize)]
pub struct LeekRenameBody<'a> {
    pub leek_id: i64,
    pub new_name: &'a str,
}

#[derive(Debug, Serialize)]
pub struct LeekSetXpBlockedBody {
    pub leek_id: i64,
    pub xp_blocked: bool,
}

impl LeekWarsClient {
    /// Full leek profile (stats, weapons, chips, etc.); public for any leek id.
    pub async fn leek_get(&self, leek_id: i64) -> Result<Value> {
        self.get_json(&format!("leek/get/{leek_id}")).await
    }

    pub async fn leek_get_count(&self) -> Result<Value> {
        self.get_json("leek/get-count").await
    }

    pub async fn leek_get_next_price(&self) -> Result<Value> {
        self.get_json("leek/get-next-price").await
    }

    pub async fn leek_create(&self, name: &str) -> Result<Value> {
        self.post_json("leek/create", &LeekCreateRequest { name })
            .await
    }

    pub async fn leek_set_in_garden(&self, leek_id: i64, in_garden: bool) -> Result<Value> {
        self.post_json(
            "leek/set-in-garden",
            &LeekSetInGardenBody { leek_id, in_garden },
        )
        .await
    }

    pub async fn leek_set_xp_blocked(&self, leek_id: i64, xp_blocked: bool) -> Result<Value> {
        self.put_json(
            "leek/set-xp-blocked",
            &LeekSetXpBlockedBody {
                leek_id,
                xp_blocked,
            },
        )
        .await
    }

    pub async fn leek_register_tournament(&self, leek_id: i64) -> Result<Value> {
        self.post_json("leek/register-tournament", &LeekIdBody { leek_id })
            .await
    }

    pub async fn leek_unregister_tournament(&self, leek_id: i64) -> Result<Value> {
        self.post_json("leek/unregister-tournament", &LeekIdBody { leek_id })
            .await
    }

    pub async fn leek_register_auto_br(&self, leek_id: i64) -> Result<Value> {
        self.post_json("leek/register-auto-br", &LeekIdBody { leek_id })
            .await
    }

    pub async fn leek_unregister_auto_br(&self, leek_id: i64) -> Result<Value> {
        self.post_json("leek/unregister-auto-br", &LeekIdBody { leek_id })
            .await
    }

    pub async fn leek_set_ai(&self, leek_id: i64, ai_id: i64) -> Result<Value> {
        self.post_json("leek/set-ai", &LeekSetAiBody { leek_id, ai_id })
            .await
    }

    pub async fn leek_remove_ai(&self, leek_id: i64) -> Result<Value> {
        self.delete_json("leek/remove-ai", &LeekIdBody { leek_id })
            .await
    }

    pub async fn leek_spend_capital(
        &self,
        leek_id: i64,
        characteristics_json: &str,
    ) -> Result<Value> {
        self.post_json(
            "leek/spend-capital",
            &LeekSpendCapitalBody {
                leek_id,
                characteristics: characteristics_json,
            },
        )
        .await
    }

    pub async fn leek_rename_habs(&self, leek_id: i64, new_name: &str) -> Result<Value> {
        self.post_json("leek/rename-habs", &LeekRenameBody { leek_id, new_name })
            .await
    }

    pub async fn leek_rename_crystals(&self, leek_id: i64, new_name: &str) -> Result<Value> {
        self.post_json(
            "leek/rename-crystals",
            &LeekRenameBody { leek_id, new_name },
        )
        .await
    }

    pub async fn leek_get_level_popup(&self, leek_id: i64) -> Result<Value> {
        self.get_json(&format!("leek/get-level-popup/{leek_id}"))
            .await
    }

    /// Move a weapon from farmer inventory onto the leek (`weapon_id` is the inventory row id).
    pub async fn leek_add_weapon(&self, leek_id: i64, inventory_weapon_id: i64) -> Result<Value> {
        self.post_json(
            "leek/add-weapon",
            &json!({ "leek_id": leek_id, "weapon_id": inventory_weapon_id }),
        )
        .await
    }

    /// Unequip a weapon (`weapon_id` is the instance id on the leek).
    pub async fn leek_remove_weapon(&self, leek_weapon_instance_id: i64) -> Result<Value> {
        self.delete_json(
            "leek/remove-weapon",
            &json!({ "weapon_id": leek_weapon_instance_id }),
        )
        .await
    }

    /// Move a chip from farmer inventory onto the leek (`chip_id` is the inventory row id).
    pub async fn leek_add_chip(&self, leek_id: i64, inventory_chip_id: i64) -> Result<Value> {
        self.post_json(
            "leek/add-chip",
            &json!({ "leek_id": leek_id, "chip_id": inventory_chip_id }),
        )
        .await
    }

    /// Unequip a chip (`chip_id` is the instance id on the leek).
    pub async fn leek_remove_chip(&self, leek_chip_instance_id: i64) -> Result<Value> {
        self.delete_json(
            "leek/remove-chip",
            &json!({ "chip_id": leek_chip_instance_id }),
        )
        .await
    }

    /// `hat_id` is the hat template id.
    pub async fn leek_set_hat(&self, leek_id: i64, hat_template_id: i64) -> Result<Value> {
        self.post_json(
            "leek/set-hat",
            &json!({ "leek_id": leek_id, "hat_id": hat_template_id }),
        )
        .await
    }

    pub async fn leek_remove_hat(&self, leek_id: i64) -> Result<Value> {
        self.delete_json("leek/remove-hat", &json!({ "leek_id": leek_id }))
            .await
    }

    /// Equip a component from inventory (`component_id` = inventory row id). `index` is the slot 0..7.
    pub async fn leek_add_component(
        &self,
        leek_id: i64,
        inventory_component_id: i64,
        index: i64,
    ) -> Result<Value> {
        self.post_json(
            "leek/add-component",
            &json!({
                "leek_id": leek_id,
                "component_id": inventory_component_id,
                "index": index,
            }),
        )
        .await
    }

    /// Unequip a component (`component_id` = instance id on the leek).
    pub async fn leek_remove_component(&self, leek_component_instance_id: i64) -> Result<Value> {
        self.delete_json(
            "leek/remove-component",
            &json!({ "component_id": leek_component_instance_id }),
        )
        .await
    }
}
