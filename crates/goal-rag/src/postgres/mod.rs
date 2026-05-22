//! PostgreSQL integration for learning from database changes
//!
//! This module provides:
//! - Connection pooling to PostgreSQL
//! - Real-time LISTEN/NOTIFY for change detection
//! - Learning pipeline that processes every INSERT/UPDATE/DELETE
//! - Integration with the existing PatternLearner and LearningSystem

pub mod config;
pub mod pool;
pub mod schema;
pub mod listener;
pub mod learner;

pub use config::PostgresConfig;
pub use pool::PgPool;
pub use listener::{ChangeListener, ChangeEvent, ChangeType};
pub use learner::DatabaseLearner;
