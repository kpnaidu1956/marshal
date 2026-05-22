# Code Quality Audit Report — fd-ruvector-marshal

**Date:** 2026-05-07
**Method:** 5-agent Ruflo swarm (Rust reviewer, TypeScript reviewer, Security auditor, Test auditor, Architecture reviewer)

---

## Priority Matrix

| # | Severity | Category | Issue | Location |
|---|----------|----------|-------|----------|
| 1 | CRITICAL | Security | Command injection via `execAsync` — user-controllable strings interpolated directly into shell commands | coordination-protocol.ts:98, regional-agent.ts:86, swarm-manager.ts:81, burst-predictor.ts:65, reactive-scaler.ts:68, capacity-manager.ts:78, burst-scaling/index.ts:231 |
| 2 | CRITICAL | Security | JWT verification bypass — `decodeJwtClaims` accepts unsigned JWTs without signature verification | external-db-proxy/index.ts:57-89 |
| 3 | CRITICAL | Security | Unauthenticated GCS uploads — no auth check + CORS wildcard | upload-to-gcs/index.ts |
| 4 | CRITICAL | Rust | `happens_before` returns true for equal clocks — breaks strict partial order of vector clocks | crates/ruvector-replication/src/conflict.rs:72 |
| 5 | CRITICAL | Rust | `MultiHeadAttention::new` panics on bad input via `assert!` — aborts Node.js process through NAPI boundary | crates/ruvector-attention/src/attention/multi_head.rs:35 |
| 6 | CRITICAL | Tests | Raft consensus and replication have zero integration tests — bugs here cause split-brain/data loss | crates/ruvector-raft, crates/ruvector-replication |
| 7 | HIGH | Security | Insecure OTP generation using `Math.random()` — predictable, not cryptographically secure | UNUSED/FD-NEW/supabase/functions/send-otp/index.ts:80, send-email-otp/index.ts:44 |
| 8 | HIGH | Security | CORS wildcard on database proxy that performs authenticated writes | UNUSED/FD-NEW/supabase/functions/external-db-proxy/index.ts:6 |
| 9 | HIGH | TypeScript | `streaming-service-optimized.ts` calls methods that don't exist on `VectorClient` — crashes at runtime | src/cloud-run/streaming-service-optimized.ts:498-501 |
| 10 | HIGH | TypeScript | Async `initialize()` called from constructors without await — race condition, silent error swallowing | coordination-protocol.ts:73, regional-agent.ts:67, swarm-manager.ts:62, agent-coordinator.ts:70 |
| 11 | HIGH | TypeScript | `updateAgentMetrics` overwrites data before reading previous value — health-change detection never fires | src/agentic-integration/agent-coordinator.ts:466-479 |
| 12 | HIGH | TypeScript | `VectorClientConfig` type mismatch — optimized service passes fields that don't exist on the config type | src/cloud-run/streaming-service-optimized.ts:425-432 |
| 13 | HIGH | Rust | Unbounded health history growth between trims — memory spikes in large clusters | crates/ruvector-replication/src/failover.rs:183/222-226 |
| 14 | HIGH | Rust | Replication log never bounds its size — `DashMap` grows indefinitely, `truncate_before` never called automatically | crates/ruvector-replication/src/sync.rs:94 |
| 15 | HIGH | Rust | `duration_since(UNIX_EPOCH).unwrap()` can panic if system clock is before epoch | crates/ruvector-collections/src/collection.rs:127-128, 154-155 |
| 16 | HIGH | Architecture | Phantom crates `ruvector-attention-cli` and `marshal-ui-react` exist on disk but are absent from workspace members — silently won't compile | Cargo.toml workspace members |
| 17 | HIGH | Tests | `ruvector-server` (HTTP/API surface) has no test files at all | crates/ruvector-server |
| 18 | HIGH | Tests | `ruvector-snapshot` (data persistence/recovery) has no integration tests | crates/ruvector-snapshot |

---

## Detailed Findings by Agent

### 1. Rust Code Quality

**CRITICAL:**
- `MultiHeadAttention::new` uses `assert!` for input validation instead of `Result`. The NAPI binding wraps this in `Result`, but the panic aborts the Node.js process before it can be caught. Must return `Result<Self, AttentionError>`.
- `happens_before` in conflict.rs returns `less || equal`, meaning equal clocks satisfy happens-before. This breaks the strict partial order. The `compare()` method masks this by checking equality first, but any direct call produces wrong causality results.

**HIGH:**
- Health history Vec in failover.rs grows unboundedly between trim cycles. In large clusters, memory spikes before trimming. The Vec also never shrinks allocation after `drain`.
- Replication log in sync.rs appends to a DashMap indefinitely. `truncate_before` exists but is never called automatically — memory leak in long-running systems.
- `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` in collection.rs panics if the system clock is set before epoch.

**MEDIUM:**
- `compute_with_mask` in multi_head.rs silently ignores the `_mask` parameter. Callers relying on masking get incorrect results with no warning.
- `StreamManager` in stream.rs never removes streams — Arc references accumulate in a Vec, leaking memory.
- Checksum in sync.rs uses `DefaultHasher` which is not stable across Rust versions and provides no integrity guarantees. Use CRC32 or xxhash.
- `DashMap` iteration in replica.rs and manager.rs is non-deterministic — causes flaky tests and inconsistent API responses.
- `evaluate_is_null` in evaluator.rs is a no-op with a TODO — always returns empty set, so `IsNull` filters silently match nothing.

**LOW:**
- Unnecessary full-struct clones on every `get_replica`/`get_primary` access.
- `parallel_attention_compute` in async_ops.rs processes queries sequentially despite "parallel" naming.
- `SetUnion` merge uses O(n*m) containment check via Vec — should use HashSet.

### 2. TypeScript Code Quality

**CRITICAL:**
- Command injection across 7+ files: `execAsync` calls interpolate user-controllable strings (agentId, region, nodeId) directly into shell commands. Example: an agentId of `"; rm -rf / #` executes arbitrary commands.
- All message/task/sync payloads typed as `any` — flows through handlers without validation, causing silent runtime errors or prototype-pollution bugs.

**HIGH:**
- Four classes call async `this.initialize()` from constructors. Since constructors can't be async, initialization errors are silently swallowed and objects may be used before ready.
- `updateAgentMetrics` overwrites `this.agentMetrics` for an agentId, then reads the "previous" metrics from the same key (which now contains the new value). Health-change detection never fires.
- `streaming-service-optimized.ts` passes config fields that don't exist on `VectorClientConfig` and calls methods (`getActiveConnections`, `getPoolSize`, `getCacheHitRate`, `getCacheSize`) that don't exist on `VectorClient`.

**MEDIUM:**
- Division by zero in cosine similarity when either vector is all zeros (regional-agent.ts:291).
- Unbounded `sentMessages` map — every sent message stored but never removed. Memory leak.
- `processMessages` called every 10ms from `setInterval` without concurrency guard — overlapping async executions.
- `Math.random() * 100` used for CPU/memory metrics driving auto-scaling decisions — non-deterministic and untestable.

### 3. Security Audit

**HIGH:**
- JWT verification bypass: `decodeJwtClaims` in external-db-proxy decodes JWT payload without signature verification. The fast path accepts any JWT with a valid `sub` and future `exp`. Attackers can forge tokens.
- Unauthenticated GCS uploads: upload-to-gcs has no authorization check and CORS is set to `*`. Anyone can upload, delete, or sync files.
- Insecure OTP generation: `Math.random()` is predictable. Use `crypto.getRandomValues()`.
- CORS wildcard on database proxy that performs authenticated writes.

**MEDIUM:**
- Legacy plain-text OTP comparison fallback in verify-otp and verify-email-otp.
- Unsafe Rust in memory pool: `from_size_align_unchecked` trusts unvalidated input. Should use checked variant.
- Phone numbers logged in cleartext in send-otp — PII leak to log aggregators.

**Positive:** No hardcoded secrets found. Credentials properly loaded from environment variables.

**Note:** HIGH security issues are in `UNUSED/FD-NEW/supabase/functions/`. Verify whether these functions are still deployed. If live, they need immediate remediation.

### 4. Test Coverage Audit

**Well-Tested:**
- `ruvector-core`: 6 test files + property-based tests (proptest), stress tests, HNSW integration. 173 inline `#[cfg(test)]` modules across the workspace.
- `ruvector-graph`: 10 test files including Cypher parsing, distributed ops, concurrency.
- `ruvector-cli`: 3 test files.
- `ruvector-attention`: inline tests + criterion benchmarks.
- Benchmark infrastructure is solid (criterion + load testing).

**Missing Tests (by risk):**

| Severity | Crate | Risk |
|----------|-------|------|
| CRITICAL | ruvector-raft | Consensus logic with zero integration tests — bugs cause split-brain/data loss |
| CRITICAL | ruvector-replication | Failover & conflict resolution untested at integration level |
| HIGH | ruvector-server | HTTP/API surface has no test files at all |
| HIGH | ruvector-snapshot | Data persistence/recovery untested — silent data loss risk |
| HIGH | ruvector-cluster | Sharding and discovery untested |
| MEDIUM | ruvector-collections | Collection management with only inline unit tests |
| MEDIUM | ruvector-filter | Query filtering with only inline tests |
| MEDIUM | ruvector-gnn | Neural network layer with only inline tests |
| MEDIUM | ruvector-tiny-dancer-core | ML model serving with 13 inline test modules but no integration tests |
| MEDIUM | goal-rag, bpe-core, goalrag-mcp | Application-level crates with no test directories |

**Weak Patterns:**
- 12+ crates rely solely on `#[cfg(test)]` inline modules — no cross-module integration tests.
- Zero error-path / fault-injection testing in raft, replication, and cluster.
- No WASM-specific tests for any `-wasm` crate.

### 5. Architecture Review

**Strengths:**
- Clean workspace-level dependency management via root `Cargo.toml` `[workspace.dependencies]`.
- Consistent platform binding triad: `-core` / `-node` (NAPI) / `-wasm` per capability.
- Good feature gating in `ruvector-core` for native vs WASM targets.
- Thin FFI boundary — no transitive leaking of internal crates.
- Dead code explicitly separated in `UNUSED/`.

**Concerns:**

| Severity | Issue |
|----------|-------|
| HIGH | Phantom crates: `ruvector-attention-cli` and `marshal-ui-react` exist on disk but excluded from workspace |
| MEDIUM | 35 workspace members with no ownership map — sprawl risk |
| MEDIUM | Dual npm workspaces (root vs npm/) creates confusion about canonical JS distribution |
| LOW | Root `src/` contains TS modules not referenced from the Rust workspace — misleads tooling |
| LOW | `dashboard/` has only `dist/` with no source or build config |

**Recommendations:**
1. Add phantom crates to workspace or move to UNUSED.
2. Consolidate to a single npm workspace root.
3. Move `src/agentic-integration`, `src/burst-scaling`, `src/cloud-run` into appropriately named packages.
4. Add CODEOWNERS manifest for the 35-crate surface.
5. Link `dashboard/` to its source or document the build pipeline.

---

## Summary Counts

| Dimension | Critical | High | Medium | Low |
|-----------|----------|------|--------|-----|
| Rust Code | 2 | 3 | 5 | 3 |
| TypeScript | 2 | 4 | 4 | 3 |
| Security | 4 | 0 | 3 | 0 |
| Tests | 2 | 3 | 6 | 1 |
| Architecture | 0 | 1 | 2 | 2 |
| **Total** | **10** | **11** | **20** | **9** |

---

## Recommended Fix Order

1. Command injection in TypeScript `execAsync` calls (exploitable now)
2. JWT verification bypass in external-db-proxy (exploitable now)
3. Unauthenticated GCS upload endpoint (exploitable now)
4. `happens_before` vector clock logic (data correctness)
5. `MultiHeadAttention::new` panic through NAPI (process crash)
6. Add integration tests for raft and replication (prevent data loss)
7. Fix `streaming-service-optimized.ts` missing methods (runtime crash)
8. Fix constructor async race conditions (silent failures)
9. Bound replication log and health history growth (memory leaks)
10. Remaining medium/low issues
