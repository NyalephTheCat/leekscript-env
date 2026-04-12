//! Teams (compositions, invitations, candidacies, emblem upload).

use serde_json::{Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    pub async fn team_get(&self, team_id: i64) -> Result<Value> {
        self.get_json(&format!("team/get/{team_id}")).await
    }

    pub async fn team_get_private(&self, team_id: i64) -> Result<Value> {
        self.get_json(&format!("team/get-private/{team_id}")).await
    }

    pub async fn team_get_connected(&self, team_id: i64) -> Result<Value> {
        self.get_json(&format!("team/get-connected/{team_id}"))
            .await
    }

    pub async fn team_get_recruiting(&self, include_closed: bool) -> Result<Value> {
        let path = format!("team/get-recruiting?include_closed={include_closed}");
        self.get_json(&path).await
    }

    pub async fn team_rankings(&self, team_id: i64) -> Result<Value> {
        self.get_json(&format!("team/rankings/{team_id}")).await
    }

    pub async fn team_create(&self, team_name: &str) -> Result<Value> {
        self.post_json("team/create", &json!({ "team_name": team_name }))
            .await
    }

    pub async fn team_send_invitation(&self, farmer_name: &str) -> Result<Value> {
        self.post_json(
            "team/send-invitation",
            &json!({ "farmer_name": farmer_name }),
        )
        .await
    }

    pub async fn team_send_candidacy(&self, team_id: i64) -> Result<Value> {
        self.post_json("team/send-candidacy", &json!({ "team_id": team_id }))
            .await
    }

    pub async fn team_cancel_candidacy_for_team(&self, team_id: i64) -> Result<Value> {
        self.post_json(
            "team/cancel-candidacy-for-team",
            &json!({ "team_id": team_id }),
        )
        .await
    }

    /// Upload a new team emblem (`team_id` + file field `emblem`, same as the web client).
    pub async fn team_set_emblem(
        &self,
        team_id: i64,
        image: Vec<u8>,
        filename: impl Into<String>,
    ) -> Result<Value> {
        let part = reqwest::multipart::Part::bytes(image).file_name(filename.into());
        let form = reqwest::multipart::Form::new()
            .text("team_id", team_id.to_string())
            .part("emblem", part);
        self.post_multipart("team/set-emblem", form).await
    }

    pub async fn team_create_composition(&self, composition_name: &str) -> Result<Value> {
        self.post_json(
            "team/create-composition",
            &json!({ "composition_name": composition_name }),
        )
        .await
    }

    pub async fn team_delete_composition(&self, composition_id: i64) -> Result<Value> {
        self.delete_json(
            "team/delete-composition",
            &json!({ "composition_id": composition_id }),
        )
        .await
    }

    pub async fn team_rename_composition(
        &self,
        composition_id: i64,
        composition_name: &str,
    ) -> Result<Value> {
        self.put_json(
            "team/rename-composition",
            &json!({
                "composition_id": composition_id,
                "composition_name": composition_name,
            }),
        )
        .await
    }

    pub async fn team_quit(&self) -> Result<Value> {
        self.post_json("team/quit", &json!({})).await
    }

    pub async fn team_dissolve(&self) -> Result<Value> {
        self.post_json("team/dissolve", &json!({})).await
    }

    pub async fn team_unregister_tournament(&self, composition_id: i64) -> Result<Value> {
        self.post_json(
            "team/unregister-tournament",
            &json!({ "composition_id": composition_id }),
        )
        .await
    }

    pub async fn team_register_tournament(&self, composition_id: i64) -> Result<Value> {
        self.post_json(
            "team/register-tournament",
            &json!({ "composition_id": composition_id }),
        )
        .await
    }

    pub async fn team_ban(&self, farmer_id: i64) -> Result<Value> {
        self.post_json("team/ban", &json!({ "farmer_id": farmer_id }))
            .await
    }

    pub async fn team_set_opened(&self, opened: bool) -> Result<Value> {
        self.post_json("team/set-opened", &json!({ "opened": opened }))
            .await
    }

    pub async fn team_set_language(&self, language: &str) -> Result<Value> {
        self.put_json("team/set-language", &json!({ "language": language }))
            .await
    }

    pub async fn team_rename_habs(&self, name: &str) -> Result<Value> {
        self.post_json("team/rename-habs", &json!({ "name": name }))
            .await
    }

    pub async fn team_rename_crystals(&self, name: &str) -> Result<Value> {
        self.post_json("team/rename-crystals", &json!({ "name": name }))
            .await
    }

    pub async fn team_change_description(&self, team_id: i64, description: &str) -> Result<Value> {
        self.put_json(
            "team/change-description",
            &json!({ "team_id": team_id, "description": description }),
        )
        .await
    }

    pub async fn team_change_recruitment_message(&self, message: &str) -> Result<Value> {
        self.put_json(
            "team/change-recruitment-message",
            &json!({ "message": message }),
        )
        .await
    }

    pub async fn team_accept_candidacy(&self, candidacy_id: i64) -> Result<Value> {
        self.post_json(
            "team/accept-candidacy",
            &json!({ "candidacy_id": candidacy_id }),
        )
        .await
    }

    pub async fn team_reject_candidacy(&self, candidacy_id: i64) -> Result<Value> {
        self.post_json(
            "team/reject-candidacy",
            &json!({ "candidacy_id": candidacy_id }),
        )
        .await
    }

    /// `columns` must be the same JSON string the web client sends (`JSON.stringify(config)`).
    pub async fn team_set_members_columns(&self, columns_json: &str) -> Result<Value> {
        self.put_json(
            "team/set-members-columns",
            &json!({ "columns": columns_json }),
        )
        .await
    }

    pub async fn team_cancel_invitation(&self, invitation_id: i64) -> Result<Value> {
        self.post_json(
            "team/cancel-invitation",
            &json!({ "invitation_id": invitation_id }),
        )
        .await
    }

    pub async fn team_accept_invitation(&self, invitation_id: i64) -> Result<Value> {
        self.post_json(
            "team/accept-invitation",
            &json!({ "invitation_id": invitation_id }),
        )
        .await
    }

    pub async fn team_reject_invitation(&self, invitation_id: i64) -> Result<Value> {
        self.post_json(
            "team/reject-invitation",
            &json!({ "invitation_id": invitation_id }),
        )
        .await
    }

    pub async fn team_change_member_grade(&self, member_id: i64, new_grade: i32) -> Result<Value> {
        self.post_json(
            "team/change-member-grade",
            &json!({ "member_id": member_id, "new_grade": new_grade }),
        )
        .await
    }

    pub async fn team_change_owner(&self, new_owner: i64, password: &str) -> Result<Value> {
        self.post_json(
            "team/change-owner",
            &json!({ "new_owner": new_owner, "password": password }),
        )
        .await
    }

    pub async fn team_move_leek(&self, leek_id: i64, to_composition_id: i64) -> Result<Value> {
        self.post_json(
            "team/move-leek",
            &json!({ "leek_id": leek_id, "to": to_composition_id }),
        )
        .await
    }

    pub async fn team_set_turret_ai(&self, ai_id: i64) -> Result<Value> {
        self.put_json("team/set-turret-ai", &json!({ "ai_id": ai_id }))
            .await
    }

    pub async fn team_set_logs_level(&self, level: i32) -> Result<Value> {
        self.put_json("team/set-logs-level", &json!({ "level": level }))
            .await
    }
}
