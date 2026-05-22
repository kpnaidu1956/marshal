# Code Quality Audit — Pass 3 (SQL, Data Types, Duplication)

**Date:** 2026-05-07
**Method:** 4-agent Ruflo swarm (SQL auditor, Rust type auditor, duplication detector, data layer auditor)
**Scope:** Full codebase with emphasis on data correctness and code redundancy

---

## Executive Summary

Pass 3 focused on data-layer correctness: SQL queries, type safety at storage boundaries, cross-tenant isolation, and duplicate implementations. This pass uncovered **3 critical auth bypasses** in goal-rag, **data loss bugs** in snapshot serialization, and **11 duplicate distance calculation implementations** that have already semantically diverged (producing different results depending on which code path is hit).

---

## 1. SQL & Database Audit

### CRITICAL

**[SQL1] format!() SQL interpolation in database.rs**
- File: `goal-rag/src/storage/database.rs:289-296`
- Code: `format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, col_type)`
- Table name, column name, and type interpolated directly into SQL. Function accepts arbitrary `&str`. Currently called with constants, but the signature is a ticking timebomb.
- **Fix**: Use allowlisted identifiers or quote-escape with a safe quoting function.

### MAJOR

**[SQL2] 6 unbounded queries — no LIMIT**
- `pg_registry.rs:256` — `list_file_records()` returns ALL rows from `rag_file_registry`
- `pg_registry.rs:449` — `list_documents()` returns ALL rows from `rag_documents`
- `pg_registry.rs:1128` — `get_all_chunk_ids()` loads ALL chunk UUIDs into memory
- `acl.rs:236` — `GroupManager::list()` returns ALL groups
- `audit/query.rs:79` — `by_resource()` returns ALL audit events for a resource
- `audit/query.rs:117` — `by_entity()` returns ALL audit events for an entity
- **Risk**: OOM or extreme latency on large tables. Audit table is partitioned and can grow very large.
- **Fix**: Add pagination with LIMIT/OFFSET or cursor-based pagination.

**[SQL3] N+1 query pattern in batch chunk insert**
- File: `pg_registry.rs:1016-1049`
- `insert_chunks_content()` executes individual INSERT per chunk in a loop.
- Contrast: `pgvector.rs` correctly uses batched multi-row VALUES.
- **Fix**: Use multi-row INSERT or COPY.

**[SQL4] Session-level SET leaks across connection pool**
- File: `pgvector.rs:276-278`
- `SET hnsw.ef_search = 100` uses session-level SET instead of `SET LOCAL`.
- Persists when connection returns to pool, affecting unrelated queries.
- **Fix**: Use `SET LOCAL` within a transaction.

### MINOR

**[SQL5] Missing organization_id scope on get_file_record**
- File: `pg_registry.rs:220-235`
- Filters by `filename` only. Filename collision across orgs returns wrong record.

**[SQL6] Hardcoded JWT secret fallback**
- File: `auth.rs:19`
- `JWT_SECRET_DEFAULT` is a constant in source. If env var unset, all tokens signed with known key.
- (Also flagged by data layer auditor — see [DL2] below)

**[SQL7] usize as i32 overflow on chunk offsets**
- Files: `pgvector.rs:244-245`, `pg_registry.rs:438-445`
- `chunk.char_start as i32` wraps on documents >2GB.

**[SQL8] FTS query injection via to_tsquery operators**
- File: `query.rs:117-152`
- User terms passed to `to_tsquery` which interprets operators (`&`, `|`, `!`). Should use `plainto_tsquery`.

### Positive

- Main query paths all use parameterized queries (`$1`, `$2`) via tokio_postgres.
- All DELETE operations include WHERE clauses with proper org_id scoping.
- pgvector search always filters by `organization_id`.
- Audit queries on partitioned table correctly use `created_at` ranges for partition pruning.

---

## 2. Rust Data Type Consistency Audit

### HIGH

**[DT1] Quantization division-by-zero and overflow**
- File: `ruvector-core/src/quantization.rs:33-37`
- When all vector values are identical, `scale = (max - min) / 255.0` becomes `0.0`. Division by zero produces `Inf`, which casts to `0u8` via undefined saturating behavior.
- **Fix**: Guard against `scale == 0.0`, return uniform quantized values.

**[DT2] Snapshot #[serde(skip)] silently drops vector payloads**
- File: `ruvector-snapshot/src/snapshot.rs:131-133`
- `payload_json` field has `#[serde(skip)]`. Any JSON-based snapshot path (REST export, debug dump) **silently loses all vector metadata**.
- **Fix**: Remove `#[serde(skip)]` or add a dedicated JSON-safe snapshot format.

### MEDIUM

**[DT3] DistanceMetric enum divergence between core and snapshot**
- `ruvector-core/src/types.rs:11`: `{Euclidean, Cosine, DotProduct, Manhattan}`
- `ruvector-snapshot/src/snapshot.rs:107`: `{Cosine, Euclidean, DotProduct}` (different order, missing Manhattan)
- Bincode serialization uses variant indices. A bincode roundtrip between these maps `Euclidean(0)` to `Cosine(0)`.
- **Fix**: Unify into a single shared type. Use `#[serde(rename)]` or named serialization.

**[DT4] f64-to-f32 precision loss in distance calculations**
- File: `ruvector-core/src/distance.rs:30-31,39,47`
- SimSIMD returns f64. Cast to f32 can change nearest-neighbor result ordering for vectors with small differences.
- **Fix**: Keep f64 through the search pipeline, convert only at API boundary.

**[DT5] Attention zip silently truncates mismatched key dimensions**
- File: `ruvector-attention/src/attention/scaled_dot_product.rs:62-98`
- Query dimension validated against `self.dim`, but individual key dimensions are not. A 128-dim query dot-producted with a 256-dim key silently uses only the first 128 dimensions.
- **Fix**: Validate each key dimension matches `self.dim`.

**[DT6] No schema versioning in redb storage**
- Files: `ruvector-core/src/storage.rs:29-30`, `agenticdb.rs:19-22`
- Tables use bincode-serialized `&[u8]` values with no version field. Struct field changes make existing databases unreadable with no migration path.
- **Fix**: Add a version byte prefix to serialized values, or use a format with schema evolution (e.g., protobuf).

### LOW

**[DT7] Unchecked `as u32` on `.len()` throughout goal-rag**
- 15+ locations: `learning.rs`, `pattern_learner.rs`, `timeline.rs`, `jobs.rs`
- Truncates silently above 4 billion. Low practical risk for analytics counts.

**[DT8] Jump-consistent-hash f64 intermediate loses precision**
- File: `ruvector-cluster/src/shard.rs:178-183`
- f64 intermediary loses precision for large i64 values (>2^53).

### Positive

- Dimension validation is thorough in `ruvector-core` storage (insert checks `vector.len() != self.dimensions`).
- `cache_optimized.rs` uses `checked_mul` for overflow-safe capacity calculations.
- Replication checksums use FNV-1a (stable across versions).

---

## 3. Duplicate Functionality Detection

### Duplication Score: 3/10 (significant redundancy, ~600-800 lines)

### CRITICAL: Distance Calculations — 11 Implementations

| # | Location | Type |
|---|----------|------|
| 1 | `ruvector-core/src/distance.rs` | SimSIMD-backed (canonical) |
| 2 | `ruvector-router-core/src/distance.rs` | Manual loop reimplementation (~90 lines) |
| 3 | `ruvector-core/src/advanced_features/mmr.rs:199` | Private `cosine_distance` |
| 4 | `ruvector-core/src/advanced/neural_hash.rs:354` | Private `cosine_similarity` |
| 5 | `ruvector-core/src/agenticdb.rs:733` | Private `euclidean_distance` |
| 6 | `ruvector-core/src/advanced/tda.rs:405` | Private `euclidean_distance` |
| 7 | `ruvector-gnn/src/search.rs:4` | Public `cosine_similarity` |
| 8 | `ruvector-attention/src/training/loss.rs:42` | Method `cosine_similarity` + `euclidean_distance` |
| 9 | `ruvector-attention/src/training/mining.rs:56` | Character-for-character copy of loss.rs |
| 10 | `ruvector-bench/src/bin/ann_benchmark.rs:235` | Private `cosine_distance` |
| 11 | `goal-rag/src/providers/entity_embeddings.rs:896` | Private `cosine_similarity` |
| 12 | `ruvector-tiny-dancer-core/src/feature_engineering.rs:134` | Method `cosine_similarity` |
| 13 | `ruvector-gnn-wasm/src/lib.rs:351` | WASM export |
| 14 | `ruvector-attention-wasm/src/utils.rs:18` | WASM export |

**Risk**: `ruvector-router-core` handles zero-norm differently from `ruvector-core` (returns 1.0 vs SimSIMD behavior). This is a **semantic divergence already producing different search results** depending on which code path handles the query.

### CRITICAL: DistanceMetric Enum — 5 Definitions

- `ruvector-core/src/types.rs:11`
- `ruvector-router-core/src/types.rs:8`
- `ruvector-router-wasm/src/lib.rs:22`
- `ruvector-router-ffi/src/lib.rs:15`
- `ruvector-snapshot/src/snapshot.rs:107`

Variant ordering differs — bincode roundtrip between core and snapshot silently maps wrong metrics.

### MAJOR: Vector Normalization — 7+ Implementations

Found in: `hnsw.rs` (test), `attention/utils.rs`, `attention-wasm/utils.rs`, `bench/lib.rs`, 2 integration test files.

### MAJOR: Softmax — 6 Implementations

Found in: `gnn/search.rs`, `attention/utils.rs`, `attention-wasm/utils.rs`, `scaled_dot_product.rs`, `mixed_curvature.rs`, `hyperbolic_attention.rs`. All use the same max-subtract-for-stability pattern.

### MAJOR: Competing Streaming Services

`streaming-service.ts` and `streaming-service-optimized.ts` are two complete implementations of the same service. The "optimized" version has module-level singletons making it untestable. Key methods in the optimized version are stubs (no-ops).

### Consolidation Recommendations

1. **Create shared `ruvector-math` module** (or expand `ruvector-core::distance`): Export all distance, normalization, and softmax functions. All crates depend on this. Savings: ~300 lines.
2. **Delete `ruvector-router-core/src/distance.rs`**: Entirely redundant.
3. **Fix internal ruvector-core imports**: `mmr.rs`, `tda.rs`, `agenticdb.rs`, `neural_hash.rs` should use `crate::distance::*`.
4. **Unify `DistanceMetric`**: Re-export from `ruvector-core` everywhere.
5. **Extract `training::metrics`** in `ruvector-attention`: Share between `loss.rs` and `mining.rs`.
6. **Merge streaming services**: Consolidate useful optimizations into the class-based `StreamingService`.

---

## 4. Goal-RAG Data Layer Audit

### CRITICAL

**[DL1] Legacy password bypass accepts ANY password**
- File: `goal-rag/src/server/routes/auth.rs:166-179`
- When a user has no `password_hash` (legacy Supabase migration), any non-empty string is accepted as valid credentials. Full authentication bypass for unmigrated accounts.
- **Fix**: Force password reset for all legacy accounts. Reject login if no hash exists.

**[DL2] Hardcoded JWT secret as fallback**
- File: `goal-rag/src/server/routes/auth.rs:19`
- `JWT_SECRET_DEFAULT = "YOUR_JWT_SECRET"` — publicly visible in source code. If `POSTGREST_JWT_SECRET` env var is unset, all tokens are forgeable.
- **Fix**: Fail-fast if JWT secret is not configured. Never use a default.

**[DL3] Password in connection string logged/serializable**
- File: `goal-rag/src/postgres/config.rs:111-115`
- `PostgresConfig` derives `Serialize` and `Debug`. The `connection_string()` method embeds the password in plaintext. Can leak to logs or serialized config dumps.
- **Fix**: Remove `Serialize`/`Debug` from config, or redact password in `Debug` impl.

### MAJOR

**[DL4] No TLS for PostgreSQL connections**
- File: `goal-rag/src/postgres/pool.rs:48`
- All connections use `NoTls`. Database traffic including passwords and query data is unencrypted.
- **Fix**: Use `tokio_postgres_rustls` or `native-tls` with proper CA verification.

**[DL5] Non-atomic document deletion**
- File: `goal-rag/src/server/routes/documents.rs:167-173`
- Removes document from in-memory registry, then deletes chunks from vector store. If vector store deletion fails, document is orphaned — removed from registry but chunks persist.
- **Fix**: Wrap in a transaction or reverse the order (delete chunks first).

**[DL6] Batch insert without transaction**
- File: `goal-rag/src/providers/pgvector.rs:397-475`
- Multi-batch insert uses a single connection but no explicit transaction. Later batch failure leaves partial data committed.
- **Fix**: Wrap in `BEGIN`/`COMMIT` transaction.

**[DL7] MCP proxy has no authentication**
- File: `goalrag-mcp/src/main.rs:186-253`
- MCP server proxies tool calls to Goal-RAG HTTP API with no auth headers. Any MCP client can execute arbitrary tools including `run_sql`.
- **Fix**: Add JWT/API key authentication to MCP proxy.

**[DL8] SET hnsw.ef_search leaks across pooled connections**
- File: `pgvector.rs:276-278`
- Duplicate of [SQL4]. Session-level SET persists in connection pool.

### MODERATE

**[DL9] Answer cache not scoped by organization**
- File: `query.rs:695`
- Cache key uses only `request.question` + document timestamps. Identical questions from different orgs return cross-tenant cached answers.
- **Fix**: Include `organization_id` in cache key.

**[DL10] Streaming endpoint skips HyDE**
- File: `query.rs:1002`
- `query_rag_v2_stream` calls `embed()` directly instead of `generate_hyde_embedding()`. Non-streaming V1/V2 use HyDE. Inconsistent retrieval quality.
- **Fix**: Use HyDE in streaming path too.

**[DL11] Auth claims optional on sensitive endpoints**
- File: `query.rs:353`
- `query_rag` accepts `claims: Option<Extension<AuthClaims>>`. When no JWT provided, default `AuthClaims` with empty `user_id` bypasses ACL enforcement.
- **Fix**: Make auth claims required. Return 401 when missing.

**[DL12] FTS query injection via to_tsquery operators**
- File: `query.rs:117-152`
- User terms passed to `to_tsquery` which interprets `&`, `|`, `!`, `<->`.
- **Fix**: Use `plainto_tsquery` (already used elsewhere at line 503).

### Positive

- Parameterized queries used throughout main paths.
- pgvector search properly scopes by organization_id.
- GCP Vertex AI calls have retry with exponential backoff.
- Embedding providers are well-abstracted behind a trait.

---

## Combined Issue Counts (Pass 3)

| Agent | Critical | High/Major | Medium/Moderate | Low/Minor |
|-------|----------|------------|-----------------|-----------|
| SQL Auditor | 1 | 3 | 1 | 4 |
| Rust Type Auditor | 0 | 2 | 4 | 2 |
| Duplication Detector | 2 | 3 | 0 | 0 |
| Data Layer Auditor | 3 | 4 | 4 | 0 |
| **Total** | **6** | **12** | **9** | **6** |

---

## Top 10 Fixes (Pass 3 Priority Order)

1. **Legacy password bypass** — any password accepted for unmigrated users [DL1]
2. **Hardcoded JWT secret fallback** — publicly known key enables token forgery [DL2]
3. **Distance calculation divergence** — 11 implementations with semantic differences producing inconsistent search results [Dup1]
4. **Snapshot serde(skip) drops vector payloads** — silent data loss on JSON export [DT2]
5. **DistanceMetric enum ordering** — bincode roundtrip maps wrong metrics between crates [DT3]
6. **MCP proxy has no authentication** — unrestricted access to run_sql [DL7]
7. **Quantization division-by-zero** — corrupts quantized data for uniform vectors [DT1]
8. **Unbounded SQL queries** — 6 queries with no LIMIT, OOM risk [SQL2]
9. **Non-atomic document deletion** — orphaned chunks on failure [DL5]
10. **Answer cache cross-tenant leakage** — org A gets org B's cached answers [DL9]

---

## Cross-Reference: All 3 Passes

### Issues confirmed across multiple passes (highest confidence):
- Hardcoded JWT secret (Pass 1 security + Pass 3 SQL + Pass 3 data layer)
- SET hnsw.ef_search pool leak (Pass 3 SQL + Pass 3 data layer)
- usize/u32 truncation patterns (Pass 1 + Pass 2 API contracts + Pass 3 types)
- f64/f32 precision loss in distances (Pass 2 performance + Pass 3 types)

### New issues found only in Pass 3:
- Legacy password bypass [DL1] — **most critical new finding**
- 11x distance calculation duplication with semantic divergence
- Snapshot data loss via serde(skip) [DT2]
- DistanceMetric bincode mismatch [DT3]
- Cross-tenant cache leakage [DL9]
- N+1 query patterns [SQL3]
- Non-atomic multi-step operations [DL5, DL6]
- MCP proxy unauthenticated [DL7]
- Auth claims optional on sensitive endpoints [DL11]
- No TLS on PostgreSQL connections [DL4]

---

## Cumulative Totals (All 3 Passes)

| Pass | Agents | Critical | High/Major | Medium | Low |
|------|--------|----------|------------|--------|-----|
| Pass 1 | 5 | 10 | 11 | 20 | 9 |
| Pass 2 | 6 | 9 | 18 | 15 | 5 |
| Pass 3 | 4 | 6 | 12 | 9 | 6 |
| **Total** | **15** | **25** | **41** | **44** | **20** |

**Grand total: 130 findings across 15 specialized agents.**
