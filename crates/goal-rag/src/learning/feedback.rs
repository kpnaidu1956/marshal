//! Feedback types for learning

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of feedback
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeedbackType {
    /// Answer was helpful
    Positive,
    /// Answer was neutral/okay
    Neutral,
    /// Answer was not helpful
    Negative,
}

impl FeedbackType {
    pub fn to_score(&self) -> i32 {
        match self {
            FeedbackType::Positive => 1,
            FeedbackType::Neutral => 0,
            FeedbackType::Negative => -1,
        }
    }
}

/// Feedback request from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// The interaction ID to provide feedback for
    pub interaction_id: Uuid,
    /// The feedback type
    pub feedback_type: FeedbackType,
    /// Optional comment
    pub comment: Option<String>,
}
