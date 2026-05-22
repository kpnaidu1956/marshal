# Architecture Review: fd-ruvector-marshal

**Date:** 2026-02-03
**Reviewer:** System Architect (Opus 4.5)
**Scope:** Full Rust workspace -- crate dependency graph, API layer, database patterns, configuration, code quality

---

## 1. Architecture Overview

### 1.1 Workspace Structure (Text Diagram)

```
fd-ruvector-marshal (workspace root)
|
+-- crates/
|   |
|   +-- [CORE LAYER]
|   |   +-- ruvector-core .............. HNSW vector DB engine, SIMD distance, quantization
|   |   +-- ruvector-collections ...... Collection management on top of core
|   |   +-- ruvector-filter ........... Metadata filtering for vector search
|   |   +-- ruvector-metrics .......... Prometheus metrics (standalone, no core dep)
|   |   +-- ruvector-snapshot ......... Point-in-time backup/restore
|   |
|   +-- [DISTRIBUTED LAYER]
|   |   +-- ruvector-cluster .......... Sharding and clustering
|   |   +-- ruvector-raft ............. Raft consensus for metadata
|   |   +-- ruvector-replication ...... Data replication and sync
|   |
|   +-- [GRAPH LAYER]
|   |   +-- ruvector-graph ............ Hypergraph DB with Cypher parsing
|   |   +-- ruvector-graph-node ....... Node.js bindings for graph
|   |   +-- ruvector-graph-wasm ....... WASM bindings for graph
|   |
|   +-- [ML LAYER]
|   |   +-- ruvector-gnn .............. Graph Neural Networks on HNSW
|   |   +-- ruvector-gnn-node ......... Node.js bindings for GNN
|   |   +-- ruvector-gnn-wasm ......... WASM bindings for GNN
|   |   +-- ruvector-attention ........ Attention mechanisms (geometric, graph, sparse)
|   |   +-- ruvector-attention-wasm ... WASM bindings for attention
|   |   +-- ruvector-attention-node ... Node.js bindings for attention
|   |
|   +-- [ROUTING LAYER]
|   |   +-- ruvector-router-core ...... Neural routing inference engine
|   |   +-- ruvector-router-cli ....... CLI for routing
|   |   +-- ruvector-router-ffi ....... FFI bindings for routing
|   |   +-- ruvector-router-wasm ...... WASM bindings for routing
|   |   +-- ruvector-tiny-dancer-core . FastGRNN agent routing
|   |   +-- ruvector-tiny-dancer-node . Node.js bindings for tiny-dancer
|   |   +-- ruvector-tiny-dancer-wasm . WASM bindings for tiny-dancer
|   |
|   +-- [BINDING LAYER]
|   |   +-- ruvector-node ............. Node.js bindings (NAPI-RS)
|   |   +-- ruvector-wasm ............. Browser WASM bindings
|   |
|   +-- [APPLICATION LAYER]
|   |   +-- ruvector-server ........... REST API server (basic, uses core)
|   |   +-- ruvector-cli .............. CLI + MCP server
|   |   +-- ruvector-bench ............ Benchmarking suite
|   |
|   +-- [RAG APPLICATION - Primary Production System]
|       +-- goal-rag .................. Full-stack RAG system
|           +-- server/ ............... Axum HTTP server
|           +-- postgres/ ............. PostgreSQL integration
|           +-- analytics/ ............ Interaction analytics
|           +-- ingestion/ ............ Document parsing pipeline
|           +-- embeddings/ ........... ONNX/Ollama embedding
|           +-- retrieval/ ............ Vector search
|           +-- generation/ ........... LLM answer generation
|           +-- storage/ .............. SQLite file registry
|           +-- learning/ ............. Knowledge store + caching
|           +-- providers/ ............ Backend abstraction (Local/GCP)
|           +-- processing/ ........... Job queue + workers
|
+-- examples/
|   +-- refrag-pipeline ............... Example RAG pipeline
|   +-- scipix ........................ Scientific paper example
|   +-- exo-ai-2025 .................. (Not in workspace, separate workspace)
|   +-- onnx-embeddings .............. (Not in workspace)
|
+-- src/ .............................. Cloud deployment configs
    +-- agentic-integration/
    +-- burst-scaling/
    +-- cloud-run/
```

### 1.2 Crate Dependency Graph

```
                    ruvector-core
                    /    |     \
                   /     |      \
        ruvector-    ruvector-    ruvector-
        collections  filter      snapshot
           \    |              /
            \   |             /
         ruvector-node   ruvector-server
         (also: metrics)

        ruvector-core
           |   \
           |    +-- ruvector-gnn
           |
           +-- ruvector-graph
                  |   |   \
                  |   |    +-- [opt] ruvector-raft
                  |   |    +-- [opt] ruvector-cluster
                  |   |    +-- [opt] ruvector-replication
                  |   |
                  +-- ruvector-graph-node
                  +-- ruvector-graph-wasm

        ruvector-core --> ruvector-cli
        ruvector-graph -> ruvector-cli
        ruvector-gnn --> ruvector-cli

        ruvector-core --> goal-rag (PRIMARY APPLICATION)
            (only depends on ruvector-core, not other crates)

        ruvector-router-core (STANDALONE - duplicates core concepts)
        ruvector-tiny-dancer-core (STANDALONE)
        ruvector-attention (STANDALONE - no core dependency)
        ruvector-metrics (STANDALONE - no core dependency)
```

---

## 2. Issues Found (Ranked by Severity)

### CRITICAL

#### C1. SQL Injection in PostgreSQL LISTEN Channel Names
**File:** `crates/goal-rag/src/postgres/listener.rs:95-96,102-103`
**Description:** Channel names for `LISTEN` commands are constructed via string formatting with no sanitization. While channel names come from config, they pass through `tables_to_listen()` which can include user-configured values from environment variables (`POSTGRES_LISTEN_TABLES`).
```rust
let channel = format!("{}_{}_changes", schema, table);
client.execute(&format!("LISTEN {}", channel), &[]).await
```
The `LISTEN` command does not use parameterized queries. If a table name or schema contains SQL metacharacters (like `;` or `--`), this could lead to SQL injection.
**Impact:** An attacker who can set the `POSTGRES_LISTEN_TABLES` or `POSTGRES_SCHEMA` environment variables could inject arbitrary SQL.
**Fix:** Validate channel names against `^[a-zA-Z_][a-zA-Z0-9_]*$` before interpolation, or use `quote_ident()` equivalent. PostgreSQL identifiers used with `LISTEN` should be validated or quoted.

#### C2. No TLS for PostgreSQL Connections
**File:** `crates/goal-rag/src/postgres/pool.rs:4,31,78`
**Description:** All PostgreSQL connections use `NoTls`:
```rust
use tokio_postgres::NoTls;
pg_config.create_pool(Some(Runtime::Tokio1), NoTls)
```
**Impact:** Database credentials and all query data travel in plaintext. In any non-localhost deployment, this is a critical security vulnerability enabling credential theft and data exfiltration via network sniffing.
**Fix:** Add TLS configuration to `PostgresConfig`. Use `tokio_postgres_rustls` or `tokio_postgres_openssl` for TLS. At minimum, support `sslmode=prefer` with `require` as the production default.

#### C3. Password in Connection String (Plaintext)
**File:** `crates/goal-rag/src/postgres/config.rs:101-105`
**Description:** The connection string is built with the password in plaintext:
```rust
pub fn connection_string(&self) -> String {
    format!(
        "host={} port={} dbname={} user={} password={}",
        self.host, self.port, self.database, self.user, self.password
    )
}
```
This string gets logged at debug level, stored in memory, and could appear in error messages or stack traces.
**Impact:** Password leakage through logs, error messages, or core dumps.
**Fix:** Use structured connection parameters instead of a connection string. If a string is needed, mask the password in any `Display`/`Debug` implementations and ensure it never reaches logs.

---

### HIGH

#### H1. Unbounded In-Memory Caches with No Eviction
**File:** `crates/goal-rag/src/server/state.rs:58-62`
**Description:** Three `DashMap` instances grow without bounds:
```rust
documents: DashMap<Uuid, Document>,
chunks: DashMap<Uuid, Chunk>,
file_registry: DashMap<String, FileRecord>,
```
Every document, chunk, and file record is cached in memory forever. Chunks contain full text content and embeddings (vectors). For a system processing thousands of documents with thousands of chunks each, this leads to OOM.
**Impact:** Memory exhaustion in production. A system processing 10,000 documents with 100 chunks each (768-dim embeddings at f32 = 3KB per vector + content) could consume 3+ GB just for the chunks cache.
**Fix:** Replace unbounded `DashMap` for `chunks` with an LRU cache (e.g., `moka` or `lru`). The chunks cache already has SQLite fallback (line 962), so eviction is safe. For `documents` and `file_registry`, add a configurable maximum size or use time-based eviction.

#### H2. Duplicate Query Logic Between v1 and v2 Endpoints
**File:** `crates/goal-rag/src/server/routes/query.rs:20-160` (v1) and `query.rs:271-470` (v2)
**Description:** `query_rag` and `query_rag_v2` contain nearly identical logic: rate limiting, validation, query type detection, embedding generation, vector search, chunk enrichment, citation building, LLM generation, and knowledge store interaction. The v2 endpoint additionally handles caching but otherwise duplicates ~150 lines.
**Impact:** Bug fixes must be applied in two places. Divergence risk is high. The v1 endpoint does NOT check the answer cache while v2 does -- this behavioral difference is likely unintentional.
**Fix:** Extract the shared RAG pipeline into a single function (e.g., `execute_rag_query()`) that returns a common result type. Let v1 and v2 handlers be thin wrappers that format the response differently.

#### H3. `file_registry_stats` Performs 4 Full Scans of DashMap
**File:** `crates/goal-rag/src/server/state.rs:1220-1234`
**Description:**
```rust
pub fn file_registry_stats(&self, organization_id: &str) -> FileRegistryStats {
    let total = self.inner.file_registry.iter().filter(...).count();
    let success = self.inner.file_registry.iter().filter(...).count();
    let failed = self.inner.file_registry.iter().filter(...).count();
    let skipped = self.inner.file_registry.iter().filter(...).count();
    ...
}
```
This iterates the entire `file_registry` DashMap four times, holding shard locks each time. With thousands of file records, this is a performance bottleneck on a concurrent system.
**Impact:** Lock contention and O(4n) scans on every `GET /api/files/stats` request.
**Fix:** Compute all four counts in a single pass:
```rust
let (mut total, mut success, mut failed, mut skipped) = (0, 0, 0, 0);
for entry in self.inner.file_registry.iter() {
    if entry.value().organization_id == organization_id {
        total += 1;
        match entry.value().status {
            FileRecordStatus::Success => success += 1,
            FileRecordStatus::Failed => failed += 1,
            FileRecordStatus::Skipped => skipped += 1,
            _ => {}
        }
    }
}
```

#### H4. `ruvector-router-core` Uses Incompatible `ndarray` Version
**File:** `crates/ruvector-router-core/Cargo.toml:31`
**Description:** While the workspace defines `ndarray = "0.16"`, `ruvector-router-core` directly specifies `ndarray = "0.15"`. This causes two versions of ndarray to be compiled, and prevents passing ndarray types between crates.
```toml
ndarray = "0.15"  # <-- should be workspace = true
```
**Impact:** Binary bloat from duplicate crate compilation. Type incompatibility if ndarray types need to cross crate boundaries.
**Fix:** Change to `ndarray = { workspace = true }` to use the workspace-defined version 0.16.

#### H5. `ruvector-attention` Does Not Use Workspace Versioning
**File:** `crates/ruvector-attention/Cargo.toml:4-8`
**Description:** This crate hardcodes `version = "0.1.0"` and `edition = "2021"` instead of using workspace inheritance. It also uses `thiserror = "1.0"` while the workspace uses `thiserror = "2.0"`.
**Impact:** Version drift. Different error handling semantics between this crate and the rest of the workspace if thiserror 1.x and 2.x coexist.
**Fix:** Switch to workspace version inheritance and workspace dependency references.

#### H6. Blocking File I/O in Async Context
**File:** `crates/goal-rag/src/server/state.rs:467-520` (`load_documents`)
**Description:** `load_documents` calls `fs::read_to_string(json_path)` (blocking I/O) within code that is ultimately called from `AppState::new()`, an async function. While the function itself is not async, it is invoked during server initialization on the async runtime.
**Impact:** Blocks the tokio runtime thread during startup. Not a runtime issue under normal conditions, but problematic if the JSON file is on a slow filesystem (NFS, network mount).
**Fix:** Use `tokio::fs::read_to_string()` within an async context, or explicitly call from `tokio::task::spawn_blocking()`.

---

### MEDIUM

#### M1. AppState Contains God Object Anti-Pattern
**File:** `crates/goal-rag/src/server/state.rs:34-78`
**Description:** `AppStateInner` holds 15+ fields including database connections, caches, providers, configuration, job queues, knowledge stores, and analytics databases. It has 50+ methods spanning 1300+ lines. This is a "God Object" that violates single responsibility.
**Impact:** Hard to test individual components. Any change to AppState risks regressions across the entire system. Mock-heavy testing required.
**Fix:** Decompose into focused service structs:
- `DocumentService` (documents DashMap, database operations)
- `FileRegistryService` (file_registry DashMap, stats, record CRUD)
- `SearchService` (vector store, embedding provider)
- `ProcessingService` (job queue, workers, external parser)
Keep `AppState` as a thin facade that delegates to these services.

#### M2. `list_documents` Loads All Documents Then Filters in Memory
**File:** `crates/goal-rag/src/server/routes/documents.rs:26-44`
**Description:**
```rust
let all_documents = state.list_documents(); // clones ALL documents
let filtered_documents: Vec<_> = all_documents
    .into_iter()
    .filter(|doc| doc.organization_id.as_ref() == Some(&query.organization_id))
    .collect();
```
This clones every document in the system into a Vec, then filters. With thousands of documents, this creates significant allocation pressure.
**Impact:** O(n) clones for every list request regardless of organization size.
**Fix:** Add a method `list_documents_by_org(org_id: &str)` that filters during iteration without cloning non-matching entries. Alternatively, use a secondary index keyed by organization_id.

#### M3. `find_by_filename` and `find_by_hash` Are O(n) Linear Scans
**File:** `crates/goal-rag/src/server/state.rs:929-944`
**Description:** Both methods iterate the entire documents DashMap to find a single entry:
```rust
self.inner.documents.iter()
    .find(|entry| entry.value().filename == filename)
    .map(|entry| entry.value().clone())
```
**Impact:** O(n) per lookup. Called during file upload deduplication, which can be frequent during bulk ingestion.
**Fix:** Maintain secondary indexes: `DashMap<String, Uuid>` for filename-to-id and `DashMap<String, Uuid>` for hash-to-id.

#### M4. `get_file_record_by_hash` Is O(n) Linear Scan
**File:** `crates/goal-rag/src/server/state.rs:1162-1168`
**Description:** Same issue as M3 but for the file registry:
```rust
self.inner.file_registry.iter()
    .find(|entry| entry.value().content_hash == content_hash)
    .map(|entry| entry.value().clone())
```
**Impact:** O(n) per hash lookup during deduplication checks on every upload.
**Fix:** Maintain a `DashMap<String, String>` mapping content_hash to filename for O(1) lookup.

#### M5. Excessive Cloning in Query Hot Path
**File:** `crates/goal-rag/src/server/routes/query.rs` (multiple locations)
**Description:** The query endpoint performs numerous clones on the hot path:
- Line 51: `request.organization_id.clone()`
- Line 52: `request.document_filter.clone()`
- Line 108: Cloning all past Q&A pairs
- Line 131: `clean_answer.clone()`, `linked_citations.clone()`
- Line 137-143: Cloning question, filenames, document IDs for the knowledge store
**Impact:** Unnecessary heap allocations on every query. For high-throughput scenarios (10+ QPS), this adds measurable latency.
**Fix:** Use `Arc<str>` for frequently shared strings. Pass references instead of clones where ownership is not needed. Consider storing the QAInteraction by moving values instead of cloning.

#### M6. CORS Allows Any Origin
**File:** `crates/goal-rag/src/server/mod.rs:40-43`
**Description:**
```rust
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);
```
**Impact:** Any website can make requests to the API, potentially leading to CSRF-like attacks if cookies or sessions are ever added. In production, this should be restricted to known frontend origins.
**Fix:** Make CORS origins configurable via `ServerConfig`. Default to restrictive in production.

#### M7. `clear_failed_files` Database Method Lacks Organization Filter
**File:** `crates/goal-rag/src/server/state.rs:1249`
**Description:**
```rust
// Clear from database (TODO: add org_id filter to database method if needed)
let db_count = match self.inner.database.clear_failed_files() {
```
The TODO comment reveals that the database-level clear operation has no organization filter. It clears ALL failed files across ALL organizations, while the in-memory cache correctly filters by org_id.
**Impact:** Multi-tenancy violation. Clearing failed files for organization A also clears them for organization B in the database.
**Fix:** Add `organization_id` parameter to `FileRegistryDb::clear_failed_files()` and include it in the SQL WHERE clause.

#### M8. WebSocket Realtime Endpoint Is Stub Implementation
**File:** `crates/goal-rag/src/server/routes/realtime.rs:1-20`
**Description:** The WebSocket endpoint is explicitly documented as a stub that tracks subscriptions but never delivers events. It is publicly exposed in the API routes.
**Impact:** Clients connecting to this endpoint will never receive database change notifications, leading to confusion and wasted connections.
**Fix:** Either complete the PostgreSQL NOTIFY integration to deliver events, or gate the endpoint behind a feature flag and return a clear error explaining it is not yet implemented.

#### M9. `ruvector-attention-cli` Exists on Disk But Not in Workspace
**File:** `crates/ruvector-attention-cli/` (directory exists, not in workspace members)
**Description:** The directory `crates/ruvector-attention-cli` exists on disk but is not listed in the workspace `Cargo.toml` members list.
**Impact:** Orphaned crate that may contain stale code. Confusing for contributors.
**Fix:** Either add it to workspace members or remove the directory.

#### M10. `profiling` Crate Exists But Not in Workspace
**File:** `crates/profiling/` (directory exists, not in workspace members)
**Description:** Similar to M9, an orphaned crate directory.
**Fix:** Add to workspace or remove.

---

### LOW

#### L1. `ruvector-router-core` Does Not Use Workspace Dependencies
**File:** `crates/ruvector-router-core/Cargo.toml:31-34`
**Description:** Uses direct version specifications instead of workspace inheritance for `ndarray`, `rand`, `uuid`, and `chrono`. This makes version management harder.
**Fix:** Convert to workspace dependency references.

#### L2. `ruvector-server` Has Hardcoded Version for Core Dependency
**File:** `crates/ruvector-server/Cargo.toml:12`
**Description:** `ruvector-core = { version = "0.1.2", path = "../ruvector-core" }` -- the version `0.1.2` is hardcoded while workspace version is `0.1.18`. Same issue in all crates that depend on core.
**Impact:** The `version` field is ignored for path dependencies within a workspace, so this is cosmetic. However, it is misleading and will cause issues if crates are published.
**Fix:** Use workspace version or remove the version field for path dependencies.

#### L3. `database.rs` at 2071 Lines Exceeds Recommended 500-Line Limit
**File:** `crates/goal-rag/src/storage/database.rs` (2071 lines)
**Description:** Violates the project's own "Files under 500 lines" guideline. Contains migration logic, CRUD for file records, documents, jobs, job files, chunks, FTS search, sync status, and stats -- all in one file.
**Fix:** Split into modules: `migrations.rs`, `file_records.rs`, `documents.rs`, `jobs.rs`, `chunks.rs`, `sync.rs`.

#### L4. `state.rs` at 1311 Lines Exceeds Recommended Limit
**File:** `crates/goal-rag/src/server/state.rs` (1311 lines)
**Fix:** Decompose as described in M1.

#### L5. `files.rs` Route Handler at 1344 Lines Exceeds Recommended Limit
**File:** `crates/goal-rag/src/server/routes/files.rs` (1344 lines)
**Fix:** Split GCP-specific upload logic into a separate module (e.g., `routes/upload.rs`).

#### L6. Dead Code: `SAFE_FILENAME_PATTERN` Is Unused
**File:** `crates/goal-rag/src/validation.rs:40-43`
**Description:** Marked `#[allow(dead_code)]` -- compiled but never called.
**Fix:** Either use it in `sanitize_filename` for stricter validation or remove it.

#### L7. Dead Code: `create_change_event` in Realtime Module
**File:** `crates/goal-rag/src/server/routes/realtime.rs:392`
**Description:** Marked `#[allow(dead_code)]` -- part of the stub implementation.
**Fix:** Remove or implement the integration.

#### L8. `pool_size` Config Not Applied to deadpool
**File:** `crates/goal-rag/src/postgres/pool.rs:18-31`
**Description:** `PostgresConfig.pool_size` is defined and configurable but is never passed to the deadpool `Config`. Deadpool uses its own default pool size.
```rust
// pool_size is read from config but never used in pg_config
pg_config.manager = Some(ManagerConfig {
    recycling_method: RecyclingMethod::Fast,
});
// Missing: pg_config.pool = Some(PoolConfig { max_size: config.pool_size, ... });
```
**Impact:** The configured pool size has no effect. Deadpool defaults may be too large or too small.
**Fix:** Set `pg_config.pool = Some(deadpool_postgres::PoolConfig::new(config.pool_size))`.

#### L9. Rate Limiter Token Refill Has Race Condition
**File:** `crates/goal-rag/src/server/middleware.rs:60-73`
**Description:** The `refill()` method acquires a write lock on `last_refill`, calculates tokens, then stores via atomic. Between the `load` and `store`, another thread could also be refilling, leading to over-refilling.
**Impact:** Under extreme concurrency, rate limits could be slightly higher than configured. Not exploitable but imprecise.
**Fix:** Use a single atomic for the refill timestamp (e.g., `AtomicU64` storing milliseconds) to avoid the write lock entirely, or make the entire refill+update atomic.

#### L10. Graceful Shutdown Not Implemented
**File:** `crates/goal-rag/src/server/mod.rs:61-80`
**Description:** `axum::serve(listener, router).await` runs until error. There is no signal handler for SIGTERM/SIGINT, no graceful drain of in-flight requests, and no cleanup of background workers (PostgreSQL listener, processing worker, job resumption).
**Impact:** In-flight requests are dropped on shutdown. Background tasks may lose buffered data.
**Fix:** Use `axum::serve(...).with_graceful_shutdown(shutdown_signal())` and implement proper cleanup for all background tasks.

#### L11. `once_cell` May Be Redundant
**File:** `Cargo.toml:104` (workspace dependency)
**Description:** `once_cell = "1.20"` is a workspace dependency, but Rust 1.80+ includes `std::sync::LazyLock` and `std::sync::OnceLock`. The workspace requires `rust-version = "1.77"`, so `once_cell` is still needed. However, when the MSRV is bumped, this can be replaced.
**Fix:** No action needed now. Track for future MSRV bump.

---

## 3. Database Interaction Patterns

### 3.1 SQLite (Primary Persistence)
- **Pattern:** Single `Mutex<Connection>` -- all SQLite access is serialized through a single mutex.
- **Location:** `crates/goal-rag/src/storage/database.rs:16-18`
- **Assessment:** Acceptable for a single-server deployment. WAL mode is correctly enabled for read concurrency. However, write throughput is limited to one writer at a time.
- **Concern:** If multiple concurrent uploads trigger database writes, the mutex becomes a bottleneck. Consider `r2d2` connection pool or multiple readers with a single writer pattern.

### 3.2 PostgreSQL (Optional)
- **Pattern:** `deadpool-postgres` connection pool with `RecyclingMethod::Fast`.
- **Assessment:** Pool configuration is incomplete (pool_size not applied, see L8). No TLS (see C2). No connection timeout configured.
- **LISTEN/NOTIFY:** Correctly uses a dedicated connection outside the pool for LISTEN. Notification channel is bounded at 100 entries.

### 3.3 Data Flow

```
Client Request
    |
    v
Axum Router -> Rate Limiter -> Route Handler
    |                              |
    |                              v
    |                     AppState (DashMap caches)
    |                         |        |
    |                         v        v
    |                    SQLite DB   Vector Store
    |                    (Mutex)     (HNSW/Vertex AI)
    |                         |
    |                         v
    |                  PostgreSQL (optional)
    |                         |
    v                         v
Response               Analytics DB (SQLite)
```

---

## 4. Configuration Management Assessment

### Strengths
- Multi-source config loading: CLI args, env vars, TOML file, defaults
- Feature flags for optional components (`gcp`, `postgres`, `cli`)
- Tiered processing configuration for different file sizes
- Rate limiting is configurable and has sensible defaults

### Weaknesses
- **No config validation at startup:** Invalid HNSW parameters (e.g., `hnsw_m = 0`) are not caught
- **No environment-specific profiles:** No distinction between dev/staging/production configs
- **Secrets in config:** PostgreSQL password can be in TOML file (should use env vars or secret manager)
- **No config hot-reload:** Rate limit changes require restart
- **Default CORS is wide open:** `allow_origin(Any)` in production

---

## 5. Summary of Recommendations (Priority Order)

| Priority | Issue | Effort | Impact |
|----------|-------|--------|--------|
| P0 | C1: SQL injection in LISTEN channels | Low | Security |
| P0 | C2: No TLS for PostgreSQL | Medium | Security |
| P0 | C3: Password in plaintext connection string | Low | Security |
| P1 | H1: Unbounded in-memory caches | Medium | Stability |
| P1 | H2: Duplicate v1/v2 query logic | Medium | Maintainability |
| P1 | H3: 4x DashMap scan in stats | Low | Performance |
| P1 | H4: ndarray version mismatch | Low | Build |
| P1 | H6: Blocking I/O in async context | Low | Correctness |
| P1 | M7: Missing org filter in clear_failed_files | Low | Multi-tenancy |
| P2 | M1: God object AppState | High | Maintainability |
| P2 | M2-M4: O(n) lookups needing indexes | Medium | Performance |
| P2 | M5: Excessive cloning in query path | Medium | Performance |
| P2 | M6: CORS wide open | Low | Security |
| P2 | L3-L5: Files exceeding 500 lines | Medium | Maintainability |
| P2 | L8: Pool size config not applied | Low | Correctness |
| P2 | L10: No graceful shutdown | Medium | Reliability |
| P3 | L1-L2: Workspace dependency hygiene | Low | Maintainability |
| P3 | L6-L7: Dead code cleanup | Low | Cleanliness |
| P3 | M8-M10: Stubs and orphans | Low | Cleanliness |

---

## 6. Architectural Strengths

1. **Clean crate separation:** The workspace correctly separates core, distributed, graph, ML, and application layers. Cross-cutting concerns (metrics, filtering) are isolated crates.

2. **Provider abstraction:** `goal-rag` uses trait-based provider abstraction (`EmbeddingProvider`, `LlmProvider`, `VectorStoreProvider`) enabling seamless switching between Local and GCP backends.

3. **Feature gating:** Optional heavy dependencies (GCP SDK, PostgreSQL) are behind feature flags, keeping the default build fast and minimal.

4. **Production hardening:** Rate limiting, circuit breaker, backpressure, concurrency limiters, and input validation are all present and well-structured.

5. **Multi-tenancy:** Organization-based isolation is consistently applied across API endpoints with proper validation.

6. **Deduplication:** Content-hash-based deduplication prevents redundant processing.

7. **Error handling:** The `Error` enum with `IntoResponse` implementation provides clean HTTP error mapping with appropriate status codes.

8. **WAL-mode SQLite:** Correct use of WAL journal mode for better concurrent read performance.
