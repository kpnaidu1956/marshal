//! Knowledge store for learning from Q&A interactions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use uuid::Uuid;

/// A stored Q&A interaction for learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAInteraction {
    pub id: Uuid,
    pub question: String,
    pub answer: String,
    pub citations_used: Vec<String>,
    pub relevance_score: f32,
    pub feedback_score: Option<i32>,  // -1, 0, or 1 from user feedback
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub document_ids: Vec<Uuid>,
}

/// Knowledge store that persists learned Q&A pairs
pub struct KnowledgeStore {
    storage_path: PathBuf,
    interactions: RwLock<HashMap<Uuid, QAInteraction>>,
    question_index: RwLock<HashMap<String, Vec<Uuid>>>,  // keyword -> interaction IDs
}

impl KnowledgeStore {
    /// Create a new knowledge store
    pub fn new(storage_path: PathBuf) -> Self {
        let store = Self {
            storage_path: storage_path.clone(),
            interactions: RwLock::new(HashMap::new()),
            question_index: RwLock::new(HashMap::new()),
        };

        // Load existing knowledge
        if let Err(e) = store.load() {
            tracing::warn!("Could not load knowledge store: {}", e);
        }

        store
    }

    /// Store a new Q&A interaction
    pub fn store_interaction(&self, interaction: QAInteraction) -> Uuid {
        let id = interaction.id;

        // Index by keywords
        let keywords = self.extract_keywords(&interaction.question);
        {
            let mut index = self.question_index.write().unwrap();
            for keyword in keywords {
                index.entry(keyword).or_default().push(id);
            }
        }

        // Store the interaction
        {
            let mut interactions = self.interactions.write().unwrap();
            interactions.insert(id, interaction);
        }

        // Persist to disk
        if let Err(e) = self.save() {
            tracing::error!("Failed to save knowledge store: {}", e);
        }

        id
    }

    /// Find similar past questions
    pub fn find_similar(&self, question: &str, limit: usize) -> Vec<QAInteraction> {
        let keywords = self.extract_keywords(question);
        let mut scores: HashMap<Uuid, usize> = HashMap::new();

        let index = self.question_index.read().unwrap();
        for keyword in &keywords {
            if let Some(ids) = index.get(keyword) {
                for id in ids {
                    *scores.entry(*id).or_default() += 1;
                }
            }
        }

        // Sort by score and return top matches
        let mut scored: Vec<_> = scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        let interactions = self.interactions.read().unwrap();
        scored
            .into_iter()
            .take(limit)
            .filter_map(|(id, _)| interactions.get(&id).cloned())
            .filter(|i| i.feedback_score.unwrap_or(0) >= 0)  // Only positive/neutral feedback
            .collect()
    }

    /// Update feedback for an interaction
    pub fn update_feedback(&self, interaction_id: Uuid, score: i32) -> bool {
        let mut interactions = self.interactions.write().unwrap();
        if let Some(interaction) = interactions.get_mut(&interaction_id) {
            interaction.feedback_score = Some(score.clamp(-1, 1));
            drop(interactions);
            let _ = self.save();
            true
        } else {
            false
        }
    }

    /// Get statistics about stored knowledge
    pub fn stats(&self) -> KnowledgeStats {
        let interactions = self.interactions.read().unwrap();
        let total = interactions.len();
        let positive = interactions.values().filter(|i| i.feedback_score == Some(1)).count();
        let negative = interactions.values().filter(|i| i.feedback_score == Some(-1)).count();

        KnowledgeStats {
            total_interactions: total,
            positive_feedback: positive,
            negative_feedback: negative,
            unique_keywords: self.question_index.read().unwrap().len(),
        }
    }

    /// Extract keywords from a question
    fn extract_keywords(&self, text: &str) -> Vec<String> {
        // Simple keyword extraction - lowercase, remove common words
        let stopwords = ["what", "is", "the", "a", "an", "and", "or", "for", "in", "on",
                         "to", "of", "are", "how", "does", "do", "can", "will", "be", "this",
                         "that", "with", "from", "by", "at", "as", "it", "its", "which"];

        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2 && !stopwords.contains(w))
            .map(|s| s.to_string())
            .collect()
    }

    /// Save to disk
    fn save(&self) -> std::io::Result<()> {
        let interactions = self.interactions.read().unwrap();
        let data = serde_json::to_string_pretty(&*interactions)?;

        // Ensure directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.storage_path, data)?;
        Ok(())
    }

    /// Load from disk
    fn load(&self) -> std::io::Result<()> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        let data = fs::read_to_string(&self.storage_path)?;
        let loaded: HashMap<Uuid, QAInteraction> = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Rebuild index
        let mut index = self.question_index.write().unwrap();
        for (id, interaction) in &loaded {
            let keywords = self.extract_keywords(&interaction.question);
            for keyword in keywords {
                index.entry(keyword).or_default().push(*id);
            }
        }

        *self.interactions.write().unwrap() = loaded;

        tracing::info!("Loaded {} Q&A interactions from knowledge store",
            self.interactions.read().unwrap().len());

        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct KnowledgeStats {
    pub total_interactions: usize,
    pub positive_feedback: usize,
    pub negative_feedback: usize,
    pub unique_keywords: usize,
}
