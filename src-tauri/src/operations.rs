use serde::{Deserialize, Serialize};

/// A pending AI-proposed operation fetched from the database.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PendingOperation {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub payload_json: String,
    pub preview_json: String,
    pub created_at: String,
    pub expires_at: String,
}
