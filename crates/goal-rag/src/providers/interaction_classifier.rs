//! Interaction classifier trait for analyzing team communications
//!
//! This trait defines the interface for classifying interactions (comments, messages)
//! into semantic categories like clarification requests, approvals, blockers, etc.

use async_trait::async_trait;
use crate::analytics::{
    ClassificationContext, ClassificationResult, InteractionSource,
};
use crate::error::Result;

/// Trait for classifying interactions using LLM
#[async_trait]
pub trait InteractionClassifier: Send + Sync {
    /// Classify a single interaction
    ///
    /// # Arguments
    /// * `content` - The text content to classify
    /// * `source` - Source type (task_comment, message, etc.)
    /// * `context` - Optional context (task title, goal title, thread history)
    ///
    /// # Returns
    /// Classification result with type, confidence, sentiment, entities
    async fn classify(
        &self,
        content: &str,
        source: InteractionSource,
        context: Option<&ClassificationContext>,
    ) -> Result<ClassificationResult>;

    /// Classify multiple interactions in batch (more efficient for LLM calls)
    ///
    /// # Arguments
    /// * `interactions` - List of (content, source) pairs to classify
    /// * `context` - Optional shared context for all interactions
    ///
    /// # Returns
    /// Vector of classification results in same order as input
    async fn classify_batch(
        &self,
        interactions: &[(String, InteractionSource)],
        context: Option<&ClassificationContext>,
    ) -> Result<Vec<ClassificationResult>>;

    /// Get the name of this classifier implementation
    fn name(&self) -> &str;

    /// Check if the classifier is available and working
    async fn health_check(&self) -> Result<bool>;
}
