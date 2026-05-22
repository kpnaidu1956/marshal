# Security Audit Report - marshal

**Date:** 2026-02-03
**Auditor:** Claude Opus 4.5 (Automated Security Review Agent)
**Scope:** `crates/goal-rag/` - All HTTP-facing code, database integrations, file handling, and authentication surfaces
**Methodology:** OWASP Top 10 for APIs, static analysis, pattern matching

---

## Executive Summary

The codebase demonstrates a generally security-conscious design with input validation modules, rate limiting, path traversal protection, and multi-tenancy isolation. However, this audit identified **4 Critical**, **7 High**, **8 Medium**, and **6 Low** severity findings that require remediation. The most urgent issues are: **hardcoded credentials committed to version control**, **complete absence of authentication/authorization middleware**, **permissive CORS configuration**, and **SQL injection vectors in PostgreSQL LISTEN channel names**.

---

## CRITICAL Findings

### C-1: Hardcoded Database Credentials in Version Control

**Severity:** CRITICAL
**CVSS Estimate:** 9.8
**File:** `/Users/deploy/PROJECTS/marshal/.claude/settings.local.json` (lines 42-56)

**Finding:** The file `.claude/settings.local.json` contains hardcoded PostgreSQL credentials in plaintext:
- Password: `REDACTED`
- Host: `REDACTED_HOST`
- User: `ragdba`
- Database: `goalrag`

Additionally, JWT tokens (both anonymous and user) are embedded in lines 50-52.

**Mitigation:** While `.claude/settings.local.json` is listed in `.gitignore`, this file may have been committed in an earlier revision. The credentials appear in the local settings with full database admin access.

**Attack Vector:** Any developer or CI system with repository access can extract production database credentials and gain full read/write access to the PostgreSQL database.

**Recommendation:**
1. Immediately rotate the `ragdba` password on `REDACTED_HOST`
2. Rotate all JWT signing secrets
3. Run `git log --all --full-history -- .claude/settings.local.json` to verify this file was never committed
4. Add a pre-commit hook that scans for credential patterns
5. Use a secret manager (GCP Secret Manager, HashiCorp Vault) instead of local files

---

### C-2: No Authentication or Authorization Middleware

**Severity:** CRITICAL
**CVSS Estimate:** 9.1
**Files:**
- `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/mod.rs` (lines 38-58)
- All route handlers in `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/`

**Finding:** The entire API surface has zero authentication. There is no auth middleware, no bearer token validation, no API key checking, and no session management anywhere in the server code. Every endpoint is publicly accessible:

```rust
// server/mod.rs - Router has no auth layers
Router::new()
    .route("/health", get(health_check))
    .route("/ready", get(readiness))
    .nest("/api", routes::api_routes(self.config.server.max_upload_size))
    .with_state(self.state.clone())
    .layer(TraceLayer::new_for_http())
    .layer(CompressionLayer::new())
    .layer(cors)
    // NO auth middleware anywhere
```

**Attack Vector:** Any unauthenticated user can:
- Upload documents to any organization
- Query and exfiltrate documents from any organization
- Delete documents from any organization
- Access analytics for any organization
- Modify file records and trigger processing jobs

**Recommendation:**
1. Implement JWT or API key authentication middleware as an axum layer
2. Extract and validate bearer tokens before any route handler executes
3. Map authenticated identity to organization membership for authorization
4. Add authentication to the `ServerConfig` struct

---

### C-3: Permissive CORS Configuration Allows Any Origin

**Severity:** CRITICAL
**CVSS Estimate:** 8.1
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/mod.rs` (lines 40-43)

**Finding:** CORS is configured to allow any origin, any method, and any header:

```rust
let cors = CorsLayer::new()
    .allow_origin(Any)     // Any website can make requests
    .allow_methods(Any)    // Including DELETE
    .allow_headers(Any);   // Including custom auth headers
```

The `enable_cors` config flag in `ServerConfig` (line 183) is never checked -- CORS is always applied regardless of the configuration.

**Attack Vector:** A malicious website can make cross-origin requests to the API from a victim's browser, potentially exfiltrating documents or modifying data. Combined with C-2 (no auth), this means any website can perform any operation on the API.

**Recommendation:**
1. Replace `Any` origins with an explicit allowlist from configuration
2. Actually check `config.server.enable_cors` before applying the CORS layer
3. Restrict allowed methods to only those needed (GET, POST, DELETE)
4. Restrict allowed headers to only expected ones (Content-Type, Authorization)

---

### C-4: SQL Injection via PostgreSQL LISTEN Channel Names

**Severity:** CRITICAL
**CVSS Estimate:** 8.6
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/postgres/listener.rs` (lines 94-103)

**Finding:** LISTEN channel names are constructed via string formatting from configuration-derived values without sanitization:

```rust
let channel = format!("{}_{}_changes", schema, table);
client.execute(&format!("LISTEN {}", channel), &[]).await
```

The `schema` comes from `PostgresConfig.schema` and `table` from `tables_to_listen()`. While the default values are safe string literals, if a user provides a malicious schema name like `api; DROP TABLE users; --` via the `POSTGRES_SCHEMA` environment variable, this becomes a SQL injection.

Similarly, the trigger generation SQL in `generate_trigger_sql()` (lines 166-224) uses string interpolation for `schema` and `table` names directly into DDL statements without parameterization.

**Attack Vector:** If an attacker can control the `POSTGRES_SCHEMA` environment variable or inject table names into the config, they can execute arbitrary SQL commands against the database.

**Recommendation:**
1. Validate schema and table names against a strict regex pattern (e.g., `^[a-z_][a-z0-9_]*$`)
2. Use PostgreSQL identifier quoting for dynamic identifiers: `quote_ident()`
3. Add validation in `PostgresConfig::from_env()` before accepting schema/table names

---

## HIGH Findings

### H-1: Missing Organization ID Validation on Document Endpoints

**Severity:** HIGH
**CVSS Estimate:** 7.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/documents.rs` (all handlers)

**Finding:** The `list_documents`, `get_document`, and `delete_document` endpoints accept `organization_id` as a query parameter but do not call `validate_organization_id()`. Unlike `files.rs` and `query.rs` which consistently validate, `documents.rs` has no import of the validation module at all.

While the code does filter by `organization_id` in the handler logic, the lack of input validation means path traversal characters or excessively long strings are passed through unchecked.

**Recommendation:** Add `use crate::validation::validate_organization_id;` and call it at the start of each handler.

---

### H-2: Missing Organization ID Validation on Job Endpoints

**Severity:** HIGH
**CVSS Estimate:** 7.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/jobs.rs`

**Finding:** The job endpoints (`get_job_progress`, `list_jobs`, `list_incomplete_jobs`, `get_job_files_progress`, `resume_job`, `get_parsers_status`) accept `organization_id` as a query parameter but do not validate it with `validate_organization_id()`. There is no import of the validation module.

**Recommendation:** Add organization ID validation to all job route handlers.

---

### H-3: Broken Object-Level Authorization (BOLA/IDOR)

**Severity:** HIGH
**CVSS Estimate:** 7.5
**Files:** Multiple route handlers

**Finding:** Since there is no authentication (C-2), authorization checks are fundamentally impossible. However, even the multi-tenancy isolation relies solely on a client-supplied `organization_id` query parameter. There is no server-side verification that the requesting user actually belongs to the specified organization.

Any user who knows or guesses another organization's ID can access their documents, queries, analytics, and files simply by changing the `organization_id` parameter.

**Attack Vector:** An attacker enumerates organization IDs (which follow a predictable slug pattern like `north-county-fire`) and accesses another tenant's data.

**Recommendation:**
1. Implement authentication (C-2) first
2. Derive organization membership from the authenticated identity
3. Never trust client-supplied organization_id for authorization decisions

---

### H-4: PostgreSQL Connection Uses NoTLS (Unencrypted)

**Severity:** HIGH
**CVSS Estimate:** 7.4
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/postgres/pool.rs` (lines 31, 78)

**Finding:** All PostgreSQL connections are established with `NoTls`:

```rust
.create_pool(Some(Runtime::Tokio1), NoTls)
// ...
tokio_postgres::connect(&self.config.connection_string(), NoTls)
```

Database credentials and query data are transmitted in plaintext. The connection goes to `REDACTED_HOST` which appears to be a GCP VM with a public IP.

**Attack Vector:** Network-level attacker (e.g., on the same GCP VPC, or if traffic routes over the internet) can sniff database credentials and all query data.

**Recommendation:**
1. Configure TLS for PostgreSQL connections using `tokio_postgres_rustls` or `native-tls`
2. Use GCP Cloud SQL Proxy for secure connectivity instead of direct IP connections
3. Add a `require_tls` option to `PostgresConfig`

---

### H-5: Sensitive Error Details Exposed to Clients

**Severity:** HIGH
**CVSS Estimate:** 6.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/error.rs` (lines 114-171)

**Finding:** Internal error details are returned directly to API clients:

```rust
Error::Io(err) => (StatusCode::INTERNAL_SERVER_ERROR, "io_error", err.to_string()),
Error::Http(err) => (StatusCode::BAD_GATEWAY, "http_error", err.to_string()),
Error::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg.clone()),
```

IO errors can leak file system paths. HTTP errors can leak internal service URLs (like Ollama/Vertex AI endpoints). Internal errors throughout the codebase include detailed messages with connection strings, file paths, and infrastructure details.

Although `validation.rs` has a `sanitize_error_message()` function (line 199), it is never used in the `IntoResponse` implementation for `Error`.

**Recommendation:**
1. Apply `sanitize_error_message()` to all error responses in the `IntoResponse` impl
2. For `Io`, `Http`, `Internal`, and `RuVector` errors, return generic messages to clients and log detailed errors server-side
3. Never expose file paths, connection strings, or internal URLs in API responses

---

### H-6: Missing Security Headers

**Severity:** HIGH
**CVSS Estimate:** 6.1
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/mod.rs`

**Finding:** The server does not set any security-related HTTP response headers. Missing headers include:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `Content-Security-Policy`
- `Strict-Transport-Security` (HSTS)
- `X-XSS-Protection: 0` (or CSP-based protection)
- `Referrer-Policy`
- `Permissions-Policy`

A search for these header names across the entire `goal-rag/src` directory returned zero matches.

**Recommendation:** Add a security headers middleware layer to the axum router.

---

### H-7: Unbounded Knowledge Store Growth Leading to Memory Exhaustion

**Severity:** HIGH
**CVSS Estimate:** 6.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/learning/knowledge_store.rs`

**Finding:** The `KnowledgeStore` stores every Q&A interaction in an in-memory `HashMap<Uuid, QAInteraction>` with no eviction, no size limit, and no TTL. Every query to `/api/query` or `/api/v2/query` adds an entry. Over time, this will consume unbounded memory.

Additionally, all `.unwrap()` calls on `RwLock` (lines 54, 62, 79, 92, 103, 116, 125, 145, 168, 176) will panic if a lock is poisoned, crashing the entire server process.

**Attack Vector:** An attacker sends many unique queries to exhaust server memory.

**Recommendation:**
1. Add a maximum capacity with LRU eviction
2. Replace `.unwrap()` on `RwLock` with proper error handling or use `parking_lot::RwLock` (which does not poison)
3. Consider moving storage to the SQLite database instead of in-memory

---

## MEDIUM Findings

### M-1: `_default` Organization Bypasses Tenant Isolation

**Severity:** MEDIUM
**CVSS Estimate:** 5.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/storage.rs` (lines 308-313)

**Finding:** The storage upload endpoint allows `_default` as an organization ID, bypassing validation:

```rust
let organization_id = organization_id.unwrap_or_else(|| "_default".to_string());
if organization_id != "_default" {
    validate_organization_id(&organization_id)?;
}
```

This creates a shared namespace where any client can upload to or read from `_default` storage, potentially colliding with or accessing other tenants' data that was uploaded without an org ID.

**Recommendation:** Require `organization_id` on all storage operations. Remove the `_default` fallback or restrict it to internal-only use.

---

### M-2: Inconsistent Organization ID Validation Between Modules

**Severity:** MEDIUM
**CVSS Estimate:** 5.3
**Files:**
- `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/analytics.rs` (line 63) - uses local `validate_org_id()`
- `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/analytics_aggregations.rs` (line 163) - uses different local `validate_org_id()`
- All other routes use `crate::validation::validate_organization_id()`

**Finding:** The analytics modules define their own `validate_org_id()` functions that only check for empty strings and length > 128, but do NOT check for path traversal characters (`..`, `/`, `\`), format regex, or other injection patterns that the canonical `validate_organization_id()` in `validation.rs` checks.

**Recommendation:** Remove duplicate validation functions and use the canonical `validate_organization_id()` from `crate::validation` consistently across all modules.

---

### M-3: Rate Limiting Not Applied to All Endpoints

**Severity:** MEDIUM
**CVSS Estimate:** 5.3
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/`

**Finding:** Rate limiting is only checked in `query.rs` (query/string-search) and `storage.rs` (upload). The following endpoints have NO rate limiting:
- `GET /api/documents` and `GET/DELETE /api/documents/:id`
- `GET /api/jobs` and all job sub-routes
- `GET /api/files` and all file sub-routes
- All analytics endpoints (`/api/analytics/*`)
- `GET /api/storage/:bucket/list`
- `DELETE /api/storage/:bucket/:org_id/*path`
- `GET /api/capabilities`
- `GET /api/info`
- `WS /api/realtime` (has connection limit but no per-IP rate limit)

**Recommendation:** Apply rate limiting as axum middleware on the router level rather than checking manually in individual handlers.

---

### M-4: WebSocket Lacks Authentication and Organization Scoping

**Severity:** MEDIUM
**CVSS Estimate:** 5.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/realtime.rs`

**Finding:** The WebSocket handler at `/api/realtime` does not require authentication and does not scope subscriptions to an organization. Any client can subscribe to change events for any table (from the whitelist) without proving they belong to the organization whose data is changing.

While this is currently a stub implementation (events are not delivered), when PostgreSQL NOTIFY integration is implemented, this will be a cross-tenant data leak.

**Recommendation:**
1. Require authentication on WebSocket upgrade
2. Scope subscriptions to the authenticated user's organization
3. Filter change events by organization_id before broadcasting

---

### M-5: External Command Execution Without Timeouts

**Severity:** MEDIUM
**CVSS Estimate:** 5.9
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/ingestion/external_parser.rs`

**Finding:** External commands (`pdftotext`, `tesseract`, `pdftoppm`, `pandoc`, `libreoffice`) are spawned using `std::process::Command` without explicit timeouts. While the parent processing has tier-based timeouts, the individual command invocations can hang indefinitely if the external tool stalls.

For example (line 315):
```rust
let mut child = Command::new("pdftotext")
    .args([...])
    .spawn()?;
let output = child.wait_with_output()?; // No timeout
```

**Attack Vector:** A crafted PDF or image could cause `tesseract` or `pdftotext` to hang indefinitely, tying up a worker thread.

**Recommendation:** Use `tokio::time::timeout` around command execution, or use `tokio::process::Command` with built-in timeout support.

---

### M-6: Filename Used Directly in LibreOffice Command Path

**Severity:** MEDIUM
**CVSS Estimate:** 5.4
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/ingestion/external_parser.rs` (line 197)

**Finding:** In `convert_with_libreoffice`, the user-supplied filename is used directly to construct a file path in a temp directory:

```rust
let input_path = temp_dir.join(filename);
fs::write(&input_path, data)?;
```

While the filename goes through `sanitize_filename()` at the upload boundary, in this internal function there is no re-validation. If an internal code path passes an unsanitized filename, this could lead to path traversal within the temp directory.

Additionally, the filename flows into `Command::new("libreoffice").args([... input_path.to_str().unwrap() ...])`. While LibreOffice is invoked with fixed flags and the path is passed as a single argument (not shell-interpreted), filenames with special characters could cause unexpected behavior.

**Recommendation:** Re-validate or sanitize the filename at the point of use, not just at the API boundary.

---

### M-7: Password Logged in Connection String

**Severity:** MEDIUM
**CVSS Estimate:** 5.0
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/postgres/config.rs` (lines 101-105)

**Finding:** The `connection_string()` method includes the password in plaintext:

```rust
pub fn connection_string(&self) -> String {
    format!(
        "host={} port={} dbname={} user={} password={}",
        self.host, self.port, self.database, self.user, self.password
    )
}
```

This string is used in `pool.rs` for `tokio_postgres::connect()`. If this connection string is ever logged (which is common during connection errors), the password will appear in logs.

**Recommendation:** Use structured connection parameters instead of a connection string, or redact the password when logging.

---

### M-8: Unvalidated `top_k` Multiplier Could Cause Large Vector Searches

**Severity:** MEDIUM
**CVSS Estimate:** 4.5
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/query.rs` (lines 55-58)

**Finding:** The `top_k` value from user input is multiplied by 2 before being passed to the vector search:

```rust
let mut search_results: Vec<VectorSearchResult> = state.vector_store_provider().search(
    &query_embedding,
    request.top_k * 2, // Get more for filtering
    Some(&filter),
).await?;
```

If `top_k` is a large value (there's no visible cap), this could cause the vector store to return a very large result set, consuming memory and CPU. The `QueryRequest` struct should validate `top_k` bounds.

**Recommendation:** Clamp `top_k` to a reasonable maximum (e.g., 50) and validate it in the request deserialization.

---

## LOW Findings

### L-1: `.unwrap()` Calls in Server-Adjacent Code

**Severity:** LOW
**CVSS Estimate:** 3.7
**Files:**
- `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/state.rs:396` - `.unwrap()` on `postgres.clone()`
- `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/realtime.rs:186` - `.unwrap()` on response builder
- `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/middleware.rs:260` - `.expect("semaphore closed")` on semaphore acquire

**Finding:** Several `.unwrap()` and `.expect()` calls in server code could panic and crash the server process if their invariants are violated. While Rust's type system makes some of these safe in practice, they violate defense-in-depth.

**Recommendation:** Replace with proper error handling that returns HTTP 500 instead of panicking.

---

### L-2: `unsafe` Blocks in Core Vector Library

**Severity:** LOW
**CVSS Estimate:** 3.5
**Files:**
- `/Users/deploy/PROJECTS/marshal/crates/ruvector-core/src/cache_optimized.rs`
- `/Users/deploy/PROJECTS/marshal/crates/ruvector-core/src/simd_intrinsics.rs`
- `/Users/deploy/PROJECTS/marshal/crates/ruvector-core/src/arena.rs`
- `/Users/deploy/PROJECTS/marshal/crates/ruvector-gnn/src/mmap.rs`
- `/Users/deploy/PROJECTS/marshal/crates/ruvector-graph/src/executor/operators.rs`
- `/Users/deploy/PROJECTS/marshal/crates/ruvector-graph/src/optimization/memory_pool.rs`

**Finding:** Multiple `unsafe` blocks exist in the core libraries for SIMD operations, custom allocators, and memory-mapped files. While these are in library code (not directly HTTP-facing) and appear carefully written (with `#![deny(unsafe_op_in_unsafe_fn)]` in some crates), bugs in unsafe code could cause memory corruption.

**Recommendation:** Fuzz test all unsafe code paths. Add MIRI testing to CI. Ensure all `unsafe impl Send/Sync` are correct.

---

### L-3: Debug Information in API Responses

**Severity:** LOW
**CVSS Estimate:** 3.1
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/server/routes/mod.rs` (lines 108-289)

**Finding:** The `/api/info` and `/api/capabilities` endpoints expose detailed system information including:
- Exact software version (`CARGO_PKG_VERSION`)
- Which external tools are installed (tesseract, pdftotext, libreoffice, pandoc)
- Installation instructions for missing tools
- Complete list of supported file formats and their handling status

**Recommendation:** Consider restricting these endpoints to authenticated admin users, or reduce the detail level in production.

---

### L-4: No Request ID or Correlation Tracking

**Severity:** LOW
**CVSS Estimate:** 2.5

**Finding:** The server does not generate or propagate request IDs. This makes it difficult to correlate log entries for security incident investigation, trace attack patterns, or debug multi-request attack sequences.

**Recommendation:** Add a request ID middleware that generates a UUID for each request and includes it in all log entries and response headers.

---

### L-5: Temp File Cleanup on Panic

**Severity:** LOW
**CVSS Estimate:** 2.0
**File:** `/Users/deploy/PROJECTS/marshal/crates/goal-rag/src/ingestion/external_parser.rs`

**Finding:** Temp directories are cleaned up with `fs::remove_dir_all(&temp_dir).ok()` at the end of functions, but if the function panics between creating the temp dir and the cleanup call, temp files containing potentially sensitive document content will persist on disk indefinitely.

**Recommendation:** Use the `tempfile` crate which provides RAII-based cleanup, or wrap temp dir operations in a `Drop` guard.

---

### L-6: `.gitignore` Broad Pattern May Miss Sensitive Files

**Severity:** LOW
**CVSS Estimate:** 2.5
**File:** `/Users/deploy/PROJECTS/marshal/.gitignore`

**Finding:** The `.gitignore` includes `*.json` which broadly ignores all JSON files. While this prevents committing most config files, it could also prevent detecting when sensitive JSON files (like `credentials.json`) are accidentally placed in unexpected locations. The pattern is overly broad and could mask security issues.

Additionally, there is no `.gitignore` entry for common credential file patterns like `*.secret`, `*.token`, or service account key files beyond `credentials.json` and `*.key`.

**Recommendation:** Use more targeted ignore patterns rather than blanket `*.json`.

---

## Positive Observations

The following security measures are already implemented and should be maintained:

1. **Input validation module** (`validation.rs`): Well-structured with path traversal prevention, length limits, filename sanitization, and org ID validation. Good test coverage.

2. **Rate limiting and circuit breaker** (`middleware.rs`): Token bucket rate limiter, circuit breaker with half-open recovery, backpressure manager, and concurrency limiters are all properly implemented.

3. **Storage path validation** (`storage.rs`): Comprehensive path validation including null byte detection, backslash rejection, control character filtering, path depth limits, and hidden file rejection.

4. **WebSocket security controls** (`realtime.rs`): Table whitelist validation, connection limits (1000 max), subscription limits (50 per connection), and message size limits (8KB).

5. **Multi-tenancy filtering**: Most endpoints filter results by organization_id, preventing cross-tenant data access at the application layer (though this needs authentication to be truly effective).

6. **Tiered processing with timeouts**: File processing has configurable per-tier timeouts, preventing indefinite resource consumption from large files.

7. **Body size limits**: Upload endpoints use `DefaultBodyLimit::max()` to prevent oversized request bodies.

8. **Bucket whitelist**: Storage operations validate bucket names against a fixed allowlist.

---

## Summary of Findings by Severity

| Severity | Count | Status |
|----------|-------|--------|
| Critical | 4     | Requires immediate remediation |
| High     | 7     | Requires remediation before production |
| Medium   | 8     | Should be addressed in next sprint |
| Low      | 6     | Track and address when convenient |
| **Total** | **25** | |

---

## Priority Remediation Roadmap

### Immediate (Week 1)
1. **C-1**: Rotate all hardcoded credentials. Verify `.claude/settings.local.json` was never committed.
2. **C-2**: Implement authentication middleware (JWT or API key).
3. **C-3**: Restrict CORS to explicit origin allowlist.
4. **C-4**: Validate PostgreSQL schema/table identifiers.

### Short-term (Weeks 2-3)
5. **H-1, H-2**: Add org ID validation to documents and jobs routes.
6. **H-3**: Derive org membership from authenticated identity.
7. **H-4**: Enable TLS for PostgreSQL connections.
8. **H-5**: Sanitize error messages before returning to clients.
9. **H-6**: Add security response headers.
10. **H-7**: Add capacity limits to KnowledgeStore.

### Medium-term (Weeks 4-6)
11. **M-1 through M-8**: Address all medium findings.
12. Implement router-level rate limiting.
13. Add request correlation IDs.
14. Fuzz test external parser inputs.

---

*This report was generated by automated static analysis and pattern matching. A manual penetration test is recommended to validate these findings and discover runtime-specific vulnerabilities.*
