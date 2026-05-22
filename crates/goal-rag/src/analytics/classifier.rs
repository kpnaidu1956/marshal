//! Ollama-based interaction classifier
//!
//! Uses the Ollama LLM to classify team interactions into semantic categories.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::analytics::{
    ClassificationContext, ClassificationResult, ExtractedEntities,
    InteractionSource, InteractionType, UrgencyLevel,
};
use crate::error::{Error, Result};
use crate::providers::interaction_classifier::InteractionClassifier;

/// Ollama-based interaction classifier
pub struct OllamaClassifier {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaClassifier {
    /// Create a new Ollama classifier
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// Create with default settings (localhost, llama3.2)
    pub fn default_local() -> Self {
        Self::new("http://localhost:11434", "llama3.2")
    }

    /// Build the classification prompt
    fn build_prompt(&self, content: &str, source: &InteractionSource, context: Option<&ClassificationContext>) -> String {
        let source_type = match source {
            InteractionSource::TaskComment => "Task Comment",
            InteractionSource::GoalComment => "Goal Comment",
            InteractionSource::Message => "Direct Message",
            InteractionSource::ActivityLog => "Activity Log Entry",
        };

        let context_str = if let Some(ctx) = context {
            let mut parts = Vec::new();
            if let Some(ref task) = ctx.task_title {
                parts.push(format!("Task: {}", task));
            }
            if let Some(ref goal) = ctx.goal_title {
                parts.push(format!("Goal: {}", goal));
            }
            if let Some(ref sender) = ctx.sender_name {
                parts.push(format!("From: {}", sender));
            }
            if parts.is_empty() {
                "No additional context".to_string()
            } else {
                parts.join("\n")
            }
        } else {
            "No additional context".to_string()
        };

        format!(r#"You are an expert at analyzing workplace communications. Classify the following interaction.

SOURCE TYPE: {}
CONTEXT:
{}

CONTENT:
{}

Classify this interaction and respond with ONLY a valid JSON object (no markdown, no explanation):
{{
    "primary_type": "<one of: request_clarification, request_resources, direction, suggestion, request_approval, status_update, acknowledgment, escalation, blocker, question, answer, assignment, feedback, recognition, other>",
    "secondary_types": [],
    "confidence": 0.0-1.0,
    "sentiment": -1.0 to 1.0,
    "urgency": "<one of: low, medium, high, critical>",
    "entities": {{
        "mentioned_users": [],
        "mentioned_deadlines": [],
        "action_items": [],
        "blockers": [],
        "resources": []
    }},
    "reasoning": "<brief 1-2 sentence explanation>"
}}"#,
            source_type, context_str, content
        )
    }

    /// Parse LLM response into ClassificationResult
    fn parse_response(&self, response: &str) -> Result<ClassificationResult> {
        // Try to extract JSON from the response
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        let parsed: LlmClassificationResponse = serde_json::from_str(json_str)
            .map_err(|e| Error::Internal(format!("Failed to parse LLM response: {} - Response: {}", e, json_str)))?;

        Ok(ClassificationResult {
            primary_type: InteractionType::parse(&parsed.primary_type),
            secondary_types: parsed.secondary_types
                .into_iter()
                .map(|s| InteractionType::parse(&s))
                .collect(),
            confidence: parsed.confidence.clamp(0.0, 1.0),
            sentiment: parsed.sentiment.clamp(-1.0, 1.0),
            urgency: UrgencyLevel::parse(&parsed.urgency),
            entities: ExtractedEntities {
                mentioned_users: parsed.entities.mentioned_users,
                mentioned_deadlines: parsed.entities.mentioned_deadlines,
                action_items: parsed.entities.action_items,
                blockers: parsed.entities.blockers,
                resources: parsed.entities.resources,
            },
            reasoning: parsed.reasoning,
        })
    }
}

#[derive(Debug, Deserialize)]
struct LlmClassificationResponse {
    primary_type: String,
    #[serde(default)]
    secondary_types: Vec<String>,
    confidence: f32,
    sentiment: f32,
    urgency: String,
    #[serde(default)]
    entities: LlmEntities,
    reasoning: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LlmEntities {
    #[serde(default)]
    mentioned_users: Vec<String>,
    #[serde(default)]
    mentioned_deadlines: Vec<String>,
    #[serde(default)]
    action_items: Vec<String>,
    #[serde(default)]
    blockers: Vec<String>,
    #[serde(default)]
    resources: Vec<String>,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[async_trait]
impl InteractionClassifier for OllamaClassifier {
    async fn classify(
        &self,
        content: &str,
        source: InteractionSource,
        context: Option<&ClassificationContext>,
    ) -> Result<ClassificationResult> {
        let prompt = self.build_prompt(content, &source, context);

        let request = OllamaRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature: 0.1, // Low temperature for consistent classification
                num_predict: 512, // Enough for JSON response
            },
        };

        let response = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Ollama request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Internal(format!(
                "Ollama returned error {}: {}", status, body
            )));
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse Ollama response: {}", e)))?;

        self.parse_response(&ollama_response.response)
    }

    async fn classify_batch(
        &self,
        interactions: &[(String, InteractionSource)],
        context: Option<&ClassificationContext>,
    ) -> Result<Vec<ClassificationResult>> {
        if interactions.is_empty() {
            return Ok(vec![]);
        }

        // Process up to 4 concurrently to avoid overwhelming Ollama
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut handles = Vec::with_capacity(interactions.len());

        for (content, source) in interactions {
            let content = content.clone();
            let source = source.clone();
            let prompt = self.build_prompt(&content, &source, context);
            let client = self.client.clone();
            let base_url = self.base_url.clone();
            let model = self.model.clone();
            let sem = semaphore.clone();

            handles.push(tokio::spawn(async move {
                let _permit = match sem.acquire().await {
                    Ok(permit) => permit,
                    Err(_) => return ClassificationResult::fallback("Semaphore closed"),
                };
                let request = OllamaRequest {
                    model,
                    prompt,
                    stream: false,
                    options: OllamaOptions {
                        temperature: 0.1,
                        num_predict: 512,
                    },
                };

                let response = client
                    .post(format!("{}/api/generate", base_url))
                    .json(&request)
                    .send()
                    .await;

                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<OllamaResponse>().await {
                            Ok(ollama_resp) => {
                                // Try to extract JSON from the response
                                let response_str = &ollama_resp.response;
                                let json_str = if let Some(start) = response_str.find('{') {
                                    if let Some(end) = response_str.rfind('}') {
                                        &response_str[start..=end]
                                    } else {
                                        response_str.as_str()
                                    }
                                } else {
                                    response_str.as_str()
                                };

                                match serde_json::from_str::<LlmClassificationResponse>(json_str) {
                                    Ok(parsed) => ClassificationResult {
                                        primary_type: InteractionType::parse(&parsed.primary_type),
                                        secondary_types: parsed.secondary_types
                                            .into_iter()
                                            .map(|s| InteractionType::parse(&s))
                                            .collect(),
                                        confidence: parsed.confidence.clamp(0.0, 1.0),
                                        sentiment: parsed.sentiment.clamp(-1.0, 1.0),
                                        urgency: UrgencyLevel::parse(&parsed.urgency),
                                        entities: ExtractedEntities {
                                            mentioned_users: parsed.entities.mentioned_users,
                                            mentioned_deadlines: parsed.entities.mentioned_deadlines,
                                            action_items: parsed.entities.action_items,
                                            blockers: parsed.entities.blockers,
                                            resources: parsed.entities.resources,
                                        },
                                        reasoning: parsed.reasoning,
                                    },
                                    Err(e) => {
                                        tracing::warn!("Failed to parse LLM response: {}", e);
                                        ClassificationResult::fallback(&format!("Parse error: {}", e))
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to decode Ollama response: {}", e);
                                ClassificationResult::fallback(&format!("Decode error: {}", e))
                            }
                        }
                    }
                    Ok(resp) => {
                        tracing::warn!("Ollama returned status {}", resp.status());
                        ClassificationResult::fallback(&format!("HTTP {}", resp.status()))
                    }
                    Err(e) => {
                        tracing::warn!("Ollama request failed: {}", e);
                        ClassificationResult::fallback(&format!("Request error: {}", e))
                    }
                }
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            results.push(handle.await.unwrap_or_else(|e| {
                ClassificationResult::fallback(&format!("Task join error: {}", e))
            }));
        }

        Ok(results)
    }

    fn name(&self) -> &str {
        "OllamaClassifier"
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Ollama health check failed: {}", e)))?;

        Ok(response.status().is_success())
    }
}

/// Rule-based fallback classifier (when LLM is unavailable)
pub struct RuleBasedClassifier;

impl RuleBasedClassifier {
    pub fn new() -> Self {
        Self
    }

    /// Simple keyword-based classification
    pub fn classify_by_keywords(&self, content: &str) -> ClassificationResult {
        let lower = content.to_lowercase();

        // Detect primary type based on keywords
        // Note: more specific patterns must come before general ones.
        // Synthesized activity content (e.g. "Status updated from blocked to in_progress")
        // must be matched early to avoid false positives from substring matches like "blocked".
        let primary_type = if lower.contains("approved by") || lower.starts_with("task approved") {
            InteractionType::Acknowledgment
        } else if lower.starts_with("status updated") || lower.starts_with("status changed") || lower.starts_with("priority changed") {
            InteractionType::StatusUpdate
        } else if lower.starts_with("task completed") || lower.contains("complet") || lower.contains("finished") || lower.contains("done") {
            InteractionType::StatusUpdate
        } else if lower.starts_with("task unblocked") || lower.contains("unblocked") || lower.contains("resolved") {
            InteractionType::StatusUpdate
        } else if lower.contains("started working") || lower.contains("in progress") {
            InteractionType::StatusUpdate
        } else if lower.contains("submitted") || lower.contains("for review") {
            InteractionType::RequestApproval
        } else if lower.contains("approve") || lower.contains("sign off") || lower.contains("greenlight") {
            InteractionType::RequestApproval
        } else if lower.contains("clarify") || lower.contains("explain") || lower.contains("what do you mean") {
            InteractionType::RequestClarification
        } else if lower.contains("blocked") || lower.contains("stuck") || lower.contains("can't proceed") {
            InteractionType::Blocker
        } else if lower.contains("escalat") {
            InteractionType::Escalation
        } else if lower.contains("status") || lower.contains("update") || lower.contains("progress") {
            InteractionType::StatusUpdate
        } else if lower.contains("suggest") || lower.contains("recommend") || lower.contains("how about") {
            InteractionType::Suggestion
        } else if lower.contains("assign") || lower.contains("take on") || lower.contains("responsible for") {
            InteractionType::Assignment
        } else if lower.ends_with('?') {
            InteractionType::Question
        } else if lower.contains("thanks") || lower.contains("got it") || lower.contains("understood") {
            InteractionType::Acknowledgment
        } else if lower.contains("great job") || lower.contains("well done") || lower.contains("excellent") {
            InteractionType::Recognition
        } else {
            InteractionType::Other
        };

        // Detect urgency
        let urgency = if lower.contains("urgent") || lower.contains("asap") || lower.contains("immediately") {
            UrgencyLevel::Critical
        } else if lower.contains("soon") || lower.contains("priority") {
            UrgencyLevel::High
        } else {
            UrgencyLevel::Medium
        };

        // Simple sentiment (avoid false negatives from status transitions like "from blocked to...")
        let sentiment = if lower.contains("great") || lower.contains("thanks") || lower.contains("good") {
            0.5
        } else if lower.contains("problem") || lower.contains("issue")
            || (lower.contains("blocked") && !lower.contains("unblocked") && !lower.starts_with("status updated"))
        {
            -0.3
        } else {
            0.0
        };

        ClassificationResult {
            primary_type,
            secondary_types: vec![],
            confidence: 0.5, // Lower confidence for rule-based
            sentiment,
            urgency,
            entities: ExtractedEntities::default(),
            reasoning: Some("Rule-based classification (LLM fallback)".to_string()),
        }
    }
}

impl Default for RuleBasedClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InteractionClassifier for RuleBasedClassifier {
    async fn classify(
        &self,
        content: &str,
        _source: InteractionSource,
        _context: Option<&ClassificationContext>,
    ) -> Result<ClassificationResult> {
        Ok(self.classify_by_keywords(content))
    }

    async fn classify_batch(
        &self,
        interactions: &[(String, InteractionSource)],
        _context: Option<&ClassificationContext>,
    ) -> Result<Vec<ClassificationResult>> {
        Ok(interactions
            .iter()
            .map(|(content, _)| self.classify_by_keywords(content))
            .collect())
    }

    fn name(&self) -> &str {
        "RuleBasedClassifier"
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true) // Always available
    }
}

/// Classifier that tries Ollama first, falls back to rule-based
pub struct HybridClassifier {
    ollama: OllamaClassifier,
    fallback: RuleBasedClassifier,
}

impl HybridClassifier {
    pub fn new(ollama_url: &str, model: &str) -> Self {
        Self {
            ollama: OllamaClassifier::new(ollama_url, model),
            fallback: RuleBasedClassifier::new(),
        }
    }

    pub fn default_local() -> Self {
        Self {
            ollama: OllamaClassifier::default_local(),
            fallback: RuleBasedClassifier::new(),
        }
    }
}

#[async_trait]
impl InteractionClassifier for HybridClassifier {
    async fn classify(
        &self,
        content: &str,
        source: InteractionSource,
        context: Option<&ClassificationContext>,
    ) -> Result<ClassificationResult> {
        match self.ollama.classify(content, source.clone(), context).await {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::warn!("Ollama classification failed, using fallback: {}", e);
                self.fallback.classify(content, source, context).await
            }
        }
    }

    async fn classify_batch(
        &self,
        interactions: &[(String, InteractionSource)],
        context: Option<&ClassificationContext>,
    ) -> Result<Vec<ClassificationResult>> {
        match self.ollama.classify_batch(interactions, context).await {
            Ok(results) => Ok(results),
            Err(e) => {
                tracing::warn!("Ollama batch classification failed, using fallback: {}", e);
                self.fallback.classify_batch(interactions, context).await
            }
        }
    }

    fn name(&self) -> &str {
        "HybridClassifier"
    }

    async fn health_check(&self) -> Result<bool> {
        // Return true if either is available
        match self.ollama.health_check().await {
            Ok(true) => Ok(true),
            _ => self.fallback.health_check().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_based_classifier() {
        let classifier = RuleBasedClassifier::new();

        // Test approval request
        let result = classifier.classify_by_keywords("Can you please approve this PR?");
        assert_eq!(result.primary_type, InteractionType::RequestApproval);

        // Test blocker
        let result = classifier.classify_by_keywords("I'm blocked on the database migration");
        assert_eq!(result.primary_type, InteractionType::Blocker);

        // Test question
        let result = classifier.classify_by_keywords("How does this work?");
        assert_eq!(result.primary_type, InteractionType::Question);

        // Test urgency
        let result = classifier.classify_by_keywords("This needs to be done ASAP!");
        assert_eq!(result.urgency, UrgencyLevel::Critical);
    }

    #[test]
    fn test_status_update_not_misclassified_as_blocker() {
        let classifier = RuleBasedClassifier::new();

        // "Status updated from blocked to in_progress" must be StatusUpdate, NOT Blocker
        let result = classifier.classify_by_keywords("Status updated from blocked to in_progress");
        assert_eq!(result.primary_type, InteractionType::StatusUpdate,
            "Status transition mentioning 'blocked' should be StatusUpdate, not Blocker");

        // Actual blockers should still classify correctly
        let result = classifier.classify_by_keywords("Task blocked by missing API credentials");
        assert_eq!(result.primary_type, InteractionType::Blocker);

        // Unblocked should be StatusUpdate
        let result = classifier.classify_by_keywords("Task unblocked by admin");
        assert_eq!(result.primary_type, InteractionType::StatusUpdate);
    }

    #[test]
    fn test_synthesized_activity_classification() {
        let classifier = RuleBasedClassifier::new();

        // Completed tasks
        let result = classifier.classify_by_keywords("Task completed by Alice");
        assert_eq!(result.primary_type, InteractionType::StatusUpdate);

        // Submitted for review → RequestApproval
        let result = classifier.classify_by_keywords("Bob submitted the task for review");
        assert_eq!(result.primary_type, InteractionType::RequestApproval);

        // Approved → Acknowledgment
        let result = classifier.classify_by_keywords("Task approved by manager");
        assert_eq!(result.primary_type, InteractionType::Acknowledgment);

        // Priority changes
        let result = classifier.classify_by_keywords("Priority changed from low to high");
        assert_eq!(result.primary_type, InteractionType::StatusUpdate);

        // Escalation
        let result = classifier.classify_by_keywords("Task escalated by team lead");
        assert_eq!(result.primary_type, InteractionType::Escalation);
    }
}
