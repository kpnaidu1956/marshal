# Interaction Analytics & Workflow Intelligence System

## Overview

This system analyzes team communications (comments, messages) attached to goals and tasks, classifies interaction types, reconstructs workflow timelines, learns patterns, and provides efficiency recommendations.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         API Layer (Axum Routes)                         │
│                    src/server/routes/analytics.rs                       │
│  POST /analysis/task/:id  GET /timeline/task/:id  GET /patterns        │
│  POST /analysis/goal/:id  GET /interactions/task  GET /recommendations │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────────────┐
│                        Analytics Module                                  │
│                      src/analytics/mod.rs                               │
├─────────────────┬─────────────────┬─────────────────┬───────────────────┤
│   classifier.rs │   timeline.rs   │pattern_learner.rs│  recommender.rs  │
│   (Ollama LLM)  │ (Reconstruct)   │ (Learn patterns) │ (Generate recs)  │
├─────────────────┴─────────────────┴─────────────────┴───────────────────┤
│                         types.rs                                         │
│  InteractionType, UrgencyLevel, WorkflowTimeline, WorkflowPattern, etc. │
├─────────────────────────────────────────────────────────────────────────┤
│                         storage.rs                                       │
│  SQLite operations for classifications, timelines, patterns, recs       │
├─────────────────────────────────────────────────────────────────────────┤
│                          jobs.rs                                         │
│  Async job processing for analysis tasks                                │
└─────────────────────────────────────────────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────────────┐
│                    Provider Abstraction Layer                            │
│              src/providers/interaction_classifier.rs                     │
│  trait InteractionClassifier { classify(), classify_batch() }           │
└─────────────────────────────────────────────────────────────────────────┘
```

## Module Structure

```
crates/goal-rag/src/
├── analytics/
│   ├── mod.rs              # Module exports (133 lines)
│   ├── types.rs            # Core types & enums (545 lines)
│   ├── classifier.rs       # Ollama + rule-based classifiers (462 lines)
│   ├── timeline.rs         # Workflow timeline reconstruction (422 lines)
│   ├── pattern_learner.rs  # Pattern learning engine (399 lines)
│   ├── recommender.rs      # Recommendation generator (453 lines)
│   ├── storage.rs          # SQLite database operations (1,244 lines)
│   └── jobs.rs             # Async job processing (703 lines)
├── providers/
│   └── interaction_classifier.rs  # Trait definition (47 lines)
└── server/routes/
    └── analytics.rs        # API endpoints (703 lines)

Total: ~4,361 lines of Rust code
```

## Data Flow

```
1. API Request (POST /analysis/task/:id)
           │
           ▼
2. Create AnalysisJob (status: pending)
           │
           ▼
3. Fetch Data from PostgreSQL
   ├── task_comments
   ├── related messages
   └── activity_logs
           │
           ▼
4. Batch Classify with LLM (Ollama llama3.2)
   └── Fallback: RuleBasedClassifier
           │
           ▼
5. Store InteractionClassifications (SQLite)
           │
           ▼
6. Reconstruct WorkflowTimeline
   ├── Merge events chronologically
   ├── Identify phases (initiated → assigned → in_progress → ...)
   └── Detect bottlenecks (approval_delay, communication_gap, etc.)
           │
           ▼
7. Match Against Learned Patterns
           │
           ▼
8. Generate EfficiencyRecommendations
           │
           ▼
9. Return Results / Mark Job Complete
```

## Database Schema

### SQLite Tables (Analytics Storage)

```sql
-- Classified interactions
CREATE TABLE IF NOT EXISTS interaction_classifications (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    source_type TEXT NOT NULL,      -- 'task_comment', 'message', 'goal_comment', 'activity_log'
    source_id TEXT NOT NULL,
    task_id TEXT,
    goal_id TEXT,
    sender_id TEXT NOT NULL,
    content TEXT NOT NULL,
    interaction_type TEXT NOT NULL, -- Primary classification
    secondary_types TEXT,           -- JSON array
    confidence_score REAL NOT NULL,
    entities TEXT,                  -- JSON: ExtractedEntities
    sentiment REAL,                 -- -1.0 to 1.0
    urgency_level TEXT,             -- 'low', 'medium', 'high', 'critical'
    references_interaction_id TEXT,
    original_created_at TEXT NOT NULL,
    classified_at TEXT NOT NULL
);

-- Reconstructed timelines
CREATE TABLE IF NOT EXISTS workflow_timelines (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,      -- 'task' or 'goal'
    entity_id TEXT NOT NULL,
    total_interactions INTEGER,
    total_participants INTEGER,
    total_duration_hours REAL,
    phases TEXT NOT NULL,           -- JSON: WorkflowPhase[]
    key_events TEXT NOT NULL,       -- JSON: TimelineEvent[]
    bottlenecks TEXT,               -- JSON: WorkflowBottleneck[]
    status TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    last_analyzed_at TEXT NOT NULL,
    UNIQUE(entity_type, entity_id)
);

-- Learned patterns
CREATE TABLE IF NOT EXISTS workflow_patterns (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    pattern_type TEXT NOT NULL,     -- 'success', 'failure', 'bottleneck', 'efficiency'
    pattern_name TEXT NOT NULL,
    description TEXT NOT NULL,
    criteria TEXT NOT NULL,         -- JSON: matching rules
    occurrence_count INTEGER,
    success_correlation REAL,
    avg_time_impact_hours REAL,
    confidence_score REAL NOT NULL,
    examples TEXT NOT NULL,         -- JSON: entity_id[]
    is_active INTEGER DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(organization_id, pattern_name)
);

-- Generated recommendations
CREATE TABLE IF NOT EXISTS efficiency_recommendations (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    target_type TEXT NOT NULL,      -- 'task', 'goal', 'team', 'organization'
    target_id TEXT,
    recommendation_type TEXT NOT NULL, -- 'process', 'communication', 'resource', 'timing'
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    suggested_actions TEXT NOT NULL, -- JSON array
    based_on_patterns TEXT NOT NULL, -- JSON: pattern_ids
    evidence TEXT NOT NULL,          -- JSON: data points
    priority TEXT NOT NULL,          -- 'low', 'medium', 'high'
    estimated_time_savings_hours REAL,
    status TEXT DEFAULT 'pending',   -- 'pending', 'accepted', 'rejected', 'implemented'
    user_feedback TEXT,
    generated_at TEXT NOT NULL
);

-- Analysis jobs
CREATE TABLE IF NOT EXISTS analysis_jobs (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    status TEXT NOT NULL,           -- 'pending', 'processing', 'completed', 'failed'
    progress_percent INTEGER DEFAULT 0,
    current_stage TEXT,
    error_message TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT
);
```

## Key Types

### InteractionType (15 types)
```rust
pub enum InteractionType {
    RequestClarification,  // Asking for more details
    RequestResources,      // Requesting tools, people, budget
    Direction,             // Giving instructions
    Suggestion,            // Proposing alternatives
    RequestApproval,       // Seeking sign-off
    StatusUpdate,          // Progress reports
    Acknowledgment,        // Confirmations
    Escalation,            // Raising to higher authority
    Blocker,               // Reporting impediments
    Question,              // General questions
    Answer,                // Responses to questions
    Assignment,            // Delegating work
    Feedback,              // Reviews and critiques
    Recognition,           // Praise and appreciation
    Other,                 // Uncategorized
}
```

### UrgencyLevel
```rust
pub enum UrgencyLevel {
    Low,       // Can wait
    Medium,    // Normal priority
    High,      // Needs attention soon
    Critical,  // Immediate action required
}
```

### PatternType
```rust
pub enum PatternType {
    Success,     // Correlated with successful outcomes
    Failure,     // Correlated with failures
    Bottleneck,  // Recurring delays
    Efficiency,  // Optimization opportunities
}
```

## Classifier Implementation

### OllamaClassifier
- Uses local Ollama LLM (default: llama3.2)
- Structured JSON output parsing
- Temperature: 0.1 for consistent results

### RuleBasedClassifier (Fallback)
- Keyword-based classification
- Always available (no external dependencies)
- Lower confidence scores (0.5)

### HybridClassifier
- Tries Ollama first
- Falls back to rule-based on failure
- Best of both worlds

## Bottleneck Detection

The system automatically detects:

1. **approval_delay**: Request to approval > 24 hours
2. **blocked_period**: Task blocked > 4 hours
3. **communication_gap**: No activity > 48 hours
4. **clarification_loop**: 3+ clarification requests

## Pattern Learning

Learns from historical data:

1. **Success Patterns**: What fast completions have in common
2. **Failure Patterns**: Common traits in failed tasks
3. **Bottleneck Patterns**: Recurring delay types
4. **Efficiency Patterns**: Optimal team sizes, timing

## Security Considerations

- Input validation on all endpoints (`validate_org_id`)
- Query limits (MAX_QUERY_LIMIT = 500)
- Generic error messages (no internal details leaked)
- Organization-scoped data access

## Configuration

Environment variables:
```bash
OLLAMA_URL=http://localhost:11434
OLLAMA_MODEL=llama3.2
ANALYTICS_DB_PATH=/path/to/analytics.db
```

## Testing

```bash
# Build and test
cargo build -p goal-rag --release
cargo test -p goal-rag

# Verify no warnings
cargo clippy -p goal-rag -- -D warnings
```
