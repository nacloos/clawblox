use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GameAction {
    Goto { position: [f32; 3] },
    Shoot { position: [f32; 3] },
    Interact { target_id: u32 },
    Wait,
}

#[derive(Debug, Clone)]
pub struct QueuedAction {
    pub agent_id: Uuid,
    pub action: GameAction,
}
