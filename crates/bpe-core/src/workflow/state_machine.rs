use crate::error::BpeError;

/// Validates step status transitions.
pub struct StepStateMachine;

impl StepStateMachine {
    /// Check whether a transition from one status to another is allowed.
    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            // pending -> ready, skipped
            ("pending", "ready") | ("pending", "skipped") |
            // ready -> in_progress, completed, skipped
            ("ready", "in_progress") | ("ready", "completed") | ("ready", "skipped") |
            // in_progress -> completed, failed, waiting_approval, waiting_integration, skipped
            ("in_progress", "completed")
            | ("in_progress", "failed")
            | ("in_progress", "waiting_approval")
            | ("in_progress", "waiting_integration")
            | ("in_progress", "skipped") |
            // waiting_approval -> in_progress, completed, failed
            ("waiting_approval", "in_progress")
            | ("waiting_approval", "completed")
            | ("waiting_approval", "failed") |
            // waiting_integration -> completed, failed
            ("waiting_integration", "completed") | ("waiting_integration", "failed") |
            // failed -> ready (retry)
            ("failed", "ready")
        )
    }

    /// Validate a transition, returning an error if disallowed.
    pub fn validate_transition(from: &str, to: &str) -> Result<(), BpeError> {
        if Self::can_transition(from, to) {
            Ok(())
        } else {
            Err(BpeError::BadRequest(format!(
                "Invalid step transition: '{from}' -> '{to}'"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert!(StepStateMachine::can_transition("pending", "ready"));
        assert!(StepStateMachine::can_transition("pending", "skipped"));
        assert!(StepStateMachine::can_transition("ready", "in_progress"));
        assert!(StepStateMachine::can_transition("in_progress", "completed"));
        assert!(StepStateMachine::can_transition("in_progress", "failed"));
        assert!(StepStateMachine::can_transition("in_progress", "waiting_approval"));
        assert!(StepStateMachine::can_transition("failed", "ready"));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!StepStateMachine::can_transition("completed", "ready"));
        assert!(!StepStateMachine::can_transition("skipped", "ready"));
        assert!(!StepStateMachine::can_transition("pending", "completed"));
    }

    #[test]
    fn test_validate_transition_err() {
        let result = StepStateMachine::validate_transition("completed", "ready");
        assert!(result.is_err());
    }
}
