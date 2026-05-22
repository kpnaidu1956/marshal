//! Answer caching with document-based invalidation
//!
//! Caches generated answers and invalidates them when cited documents change.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Cached answer with metadata
#[derive(Debug, Clone)]
pub struct CachedAnswer {
    /// Original question
    pub question: String,
    /// Generated answer
    pub answer: String,
    /// Document IDs cited in this answer
    pub cited_document_ids: Vec<Uuid>,
    /// Document timestamps at cache time (for staleness detection)
    pub document_timestamps: HashMap<Uuid, DateTime<Utc>>,
    /// When this was cached
    pub cached_at: DateTime<Utc>,
    /// Number of cache hits
    pub hit_count: u32,
    /// The citations stored with the answer
    pub citations: Vec<CachedCitation>,
}

/// Minimal citation info for caching
#[derive(Debug, Clone)]
pub struct CachedCitation {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub filename: String,
    pub snippet: String,
    pub similarity_score: f32,
}

/// Answer cache with document-based invalidation
pub struct AnswerCache {
    /// Cache entries keyed by question hash
    cache: RwLock<HashMap<String, CachedAnswer>>,
    /// Reverse index: document_id -> questions that cite it
    doc_to_questions: RwLock<HashMap<Uuid, HashSet<String>>>,
    /// Maximum cache size
    max_entries: usize,
    /// TTL for cache entries (seconds)
    ttl_seconds: u64,
}

impl AnswerCache {
    /// Create a new answer cache
    pub fn new(max_entries: usize, ttl_seconds: u64) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            doc_to_questions: RwLock::new(HashMap::new()),
            max_entries,
            ttl_seconds,
        }
    }

    /// Hash a question + organization_id for cache key (prevents cross-tenant leakage)
    fn hash_question(question: &str, organization_id: &str) -> String {
        let normalized = question.to_lowercase().trim().to_string();
        let mut hasher = Sha256::new();
        hasher.update(organization_id.as_bytes());
        hasher.update(b":");
        hasher.update(normalized.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get a cached answer if valid
    ///
    /// Returns None if:
    /// - Not in cache
    /// - TTL expired
    /// - Any cited document has been modified (timestamp changed)
    pub fn get(
        &self,
        question: &str,
        organization_id: &str,
        current_timestamps: &HashMap<Uuid, DateTime<Utc>>,
    ) -> Option<CachedAnswer> {
        let key = Self::hash_question(question, organization_id);
        let mut cache = self.cache.write();

        if let Some(entry) = cache.get_mut(&key) {
            // Check TTL
            let age = Utc::now().signed_duration_since(entry.cached_at);
            if age.num_seconds() as u64 > self.ttl_seconds {
                tracing::debug!("Cache miss (TTL expired): {}", &key[..12]);
                cache.remove(&key);
                return None;
            }

            // Check if any cited document has been modified
            for doc_id in &entry.cited_document_ids {
                let cached_ts = entry.document_timestamps.get(doc_id);
                let current_ts = current_timestamps.get(doc_id);

                match (cached_ts, current_ts) {
                    // Document deleted or not found
                    (Some(_), None) => {
                        tracing::debug!("Cache miss (document {} deleted)", doc_id);
                        cache.remove(&key);
                        return None;
                    }
                    // Document modified
                    (Some(cached), Some(current)) if cached != current => {
                        tracing::debug!("Cache miss (document {} modified)", doc_id);
                        cache.remove(&key);
                        return None;
                    }
                    _ => {}
                }
            }

            // Cache hit
            entry.hit_count += 1;
            tracing::debug!("Cache hit: {} (hits: {})", &key[..12], entry.hit_count);
            return Some(entry.clone());
        }

        None
    }

    /// Store an answer in the cache
    pub fn put(
        &self,
        question: &str,
        organization_id: &str,
        answer: String,
        citations: Vec<CachedCitation>,
        doc_timestamps: HashMap<Uuid, DateTime<Utc>>,
    ) {
        let key = Self::hash_question(question, organization_id);
        let doc_ids: Vec<Uuid> = citations.iter().map(|c| c.document_id).collect();

        let entry = CachedAnswer {
            question: question.to_string(),
            answer,
            cited_document_ids: doc_ids.clone(),
            document_timestamps: doc_timestamps,
            cached_at: Utc::now(),
            hit_count: 0,
            citations,
        };

        // Evict oldest entries if at capacity
        {
            let mut cache = self.cache.write();
            if cache.len() >= self.max_entries {
                // Find oldest entry
                if let Some(oldest_key) = cache
                    .iter()
                    .min_by_key(|(_, v)| v.cached_at)
                    .map(|(k, _)| k.clone())
                {
                    cache.remove(&oldest_key);
                }
            }
            cache.insert(key.clone(), entry);
        }

        // Update reverse index
        {
            let mut doc_to_q = self.doc_to_questions.write();
            for doc_id in doc_ids {
                doc_to_q
                    .entry(doc_id)
                    .or_default()
                    .insert(key.clone());
            }
        }

        tracing::debug!("Cached answer: {}", &key[..12]);
    }

    /// Invalidate all cache entries that cite a specific document
    ///
    /// Called when a document is modified or deleted
    pub fn invalidate_by_document(&self, doc_id: &Uuid) -> usize {
        let question_keys: Vec<String> = {
            let doc_to_q = self.doc_to_questions.read();
            doc_to_q
                .get(doc_id)
                .map(|keys| keys.iter().cloned().collect())
                .unwrap_or_default()
        };

        if question_keys.is_empty() {
            return 0;
        }

        let mut cache = self.cache.write();
        let mut invalidated = 0;

        for key in &question_keys {
            if cache.remove(key).is_some() {
                invalidated += 1;
            }
        }

        // Clean up reverse index
        {
            let mut doc_to_q = self.doc_to_questions.write();
            doc_to_q.remove(doc_id);
        }

        if invalidated > 0 {
            tracing::info!(
                "Invalidated {} cached answers for document {}",
                invalidated,
                doc_id
            );
        }

        invalidated
    }

    /// Clear the entire cache
    pub fn clear(&self) {
        self.cache.write().clear();
        self.doc_to_questions.write().clear();
        tracing::info!("Answer cache cleared");
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read();
        let total_hits: u32 = cache.values().map(|e| e.hit_count).sum();

        CacheStats {
            entries: cache.len(),
            total_hits,
            max_entries: self.max_entries,
            ttl_seconds: self.ttl_seconds,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub entries: usize,
    pub total_hits: u32,
    pub max_entries: usize,
    pub ttl_seconds: u64,
}

impl Default for AnswerCache {
    fn default() -> Self {
        Self::new(1000, 3600) // 1000 entries, 1 hour TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit() {
        let cache = AnswerCache::new(10, 3600);
        let doc_id = Uuid::new_v4();
        let now = Utc::now();

        let mut timestamps = HashMap::new();
        timestamps.insert(doc_id, now);

        let citations = vec![CachedCitation {
            chunk_id: Uuid::new_v4(),
            document_id: doc_id,
            filename: "test.pdf".to_string(),
            snippet: "test content".to_string(),
            similarity_score: 0.9,
        }];

        cache.put("What is the policy?", "org-1", "The policy states...".to_string(), citations, timestamps.clone());

        let result = cache.get("What is the policy?", "org-1", &timestamps);
        assert!(result.is_some());
        assert_eq!(result.unwrap().answer, "The policy states...");
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = AnswerCache::new(10, 3600);
        let doc_id = Uuid::new_v4();
        let now = Utc::now();

        let mut timestamps = HashMap::new();
        timestamps.insert(doc_id, now);

        let citations = vec![CachedCitation {
            chunk_id: Uuid::new_v4(),
            document_id: doc_id,
            filename: "test.pdf".to_string(),
            snippet: "test content".to_string(),
            similarity_score: 0.9,
        }];

        cache.put("What is the policy?", "org-1", "The policy states...".to_string(), citations, timestamps);

        // Invalidate by document
        let invalidated = cache.invalidate_by_document(&doc_id);
        assert_eq!(invalidated, 1);

        // Should no longer be cached
        let mut new_timestamps = HashMap::new();
        new_timestamps.insert(doc_id, now);
        let result = cache.get("What is the policy?", "org-1", &new_timestamps);
        assert!(result.is_none());
    }
}
