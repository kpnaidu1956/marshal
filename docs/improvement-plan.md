# Ruvector Improvement Plan — Mar 12, 2026

Comprehensive review by Architect, Designer, and Rust Expert agents.

---

## Tier 1 — Performance (Rust Expert)

| ID | Task | Impact | Status |
|----|------|--------|--------|
| P1 | Batch chunk INSERTs (single multi-row INSERT instead of N round-trips) | 10-50x ingestion | DONE (Mar 12) |
| P2 | Parallelize Ollama `embed_batch` with `buffer_unordered(8)` | 4-8x embedding | DONE (Mar 12) |
| P3 | Rewrite backfill `NOT IN` to `LEFT JOIN ... IS NULL` | 2-10x backfill | DONE (Mar 12) |
| P4 | Increase default connection pool size from 5 to 15 | Prevents pool starvation | DONE (Mar 12) |
| P5 | Cache org slug-to-UUID resolution (LazyLock+RwLock, 60s TTL) | Eliminates redundant queries | DONE (Mar 12) |

### Additional Performance Items

| ID | Task | Impact | Status |
|----|------|--------|--------|
| P6 | Replace `async_trait` with native async traits (Rust 1.77+) | Removes boxing overhead | TODO |
| P7 | Use simsimd for cosine similarity in entity_embeddings.rs | 4-8x faster similarity | TODO |
| P8 | Gate SQLite deps behind feature flag (saves build time) | ~30s build time | TODO |
| P9 | Replace `stamp_source_tool` 2s sleep with mpsc channel batch | Reduces sleeping tasks | TODO |
| P10 | Extract shared v1/v2 query pipeline to `QueryPipeline` struct | Maintenance, not perf | TODO |
| P11 | Combine FK validation queries in write tools into single query | 3 round-trips to 1 | TODO |
| P12 | Use `lto = "thin"` for dev release builds | ~30-60s build time | TODO |
| P13 | Set pgvector `ef_search` at query time based on top_k | Better recall tuning | TODO |
| P14 | Use `plainto_tsquery` instead of manual `to_tsquery` with OR | Prevents FTS parse errors | TODO |

---

## Tier 1 — RAG Quality (Architect)

| ID | Task | Impact | Status |
|----|------|--------|--------|
| R1 | Add cross-encoder reranker after retrieval (LLM-based, parallel scoring) | Biggest RAG quality win | DONE (Mar 12) |
| R2 | Tune pgvector HNSW (`ef_construction=200`, set `ef_search=100` at query time) | Free quality/perf win | DONE (Mar 12) |
| R3 | Implement HyDE query rewriting (~50 lines in query.rs) | Better retrieval for complex queries | DONE (Mar 12) |
| R4 | Add LLM response streaming (`generate_stream` on LlmProvider trait) | Dramatically better UX | DONE (Mar 12) |
| R5 | Eliminate PostgREST (tools already query api schema directly) | Removes operational burden | TODO |

### Additional RAG/Architecture Items

| ID | Task | Impact | Status |
|----|------|--------|--------|
| R6 | Switch to better embedding model (Voyage-3-large or Cohere embed-v4) | Better retrieval quality | TODO |
| R7 | Implement semantic chunking (embedding similarity-based splits) | Better chunk coherence | TODO |
| R8 | Add database migrations (sqlx-migrate or refinery) | Prevents schema drift | TODO |
| R9 | Move KnowledgeStore from file-based HashMap to PostgreSQL | Durability, semantic matching | TODO |
| R10 | Add MCP resources and prompts to goalrag-mcp | Better AI integration | TODO |
| R11 | Add Claude as alternative LLM backend | Better citation grounding | TODO |
| R12 | Add organization-specific partial HNSW indexes | Performance at scale | TODO |
| R13 | Add OpenTelemetry tracing | Production debugging | TODO |
| R14 | Containerize deployment (Dockerfile + CI/CD) | Reproducibility | TODO |
| R15 | Add chunk metadata enrichment (heading hierarchy) | Better citations | TODO |
| R16 | Multi-step agentic RAG for complex queries | Quality for hard questions | TODO |

---

## Tier 1 — UX (Designer)

| ID | Task | Impact | Status |
|----|------|--------|--------|
| U1 | Add `/tasks` list page with filtering and sorting | Critical — missing entirely | DONE (Mar 12) |
| U2 | Responsive/collapsible sidebar + mobile hamburger menu | Critical — unusable on mobile | DONE (Mar 12) |
| U3 | Cmd+K command palette for global search | High — leverages existing search tools | DONE (Mar 12) |
| U4 | Add create/edit forms for tasks and goals | High — currently impossible via UI | TODO |
| U5 | Dark mode toggle + persist preference | High — all dark: variants exist but unused | DONE (Mar 12) |

### Additional UX Items

| ID | Task | Impact | Status |
|----|------|--------|--------|
| U6 | Fix dropdowns to close on outside click | High, low effort | DONE (Mar 12) |
| U7 | Extract shared components (Badge, StatusBadge, EmptyState, Skeleton) | Medium | TODO |
| U8 | Add table pagination, sorting, column filtering to DataTable | Medium | TODO |
| U9 | Remove unused thaw dependency from marshal-ui | Medium, low effort | TODO |
| U10 | Add AI assistant panel accessible from any page | Medium, high effort | TODO |
| U11 | Fix N+1 HTTP requests on workload/team pages | Medium | TODO |
| U12 | Add breadcrumb navigation for detail pages | Medium | TODO |
| U13 | Add ARIA labels and keyboard navigation | Lower | TODO |
| U14 | Define missing `animate-slide-in` CSS keyframes for toasts | Lower | TODO |
| U15 | Restructure sidebar navigation (group Team, Calendar subsections) | Lower | TODO |

---

## Progress Log

| Date | Changes |
|------|---------|
| 2026-03-12 | Plan created from Architect + Designer + Rust Expert reviews |
| 2026-03-12 | P1-P5 implemented: batch INSERTs, parallel embeddings, LEFT JOIN backfill, pool 15, org cache |
| 2026-03-12 | R2-R4 implemented: HNSW tuning, HyDE query rewriting, SSE streaming endpoint |
| 2026-03-12 | R1 implemented: LLM-based cross-encoder reranker with parallel scoring |
| 2026-03-12 | U1-U3, U5-U6 implemented: Tasks page, responsive sidebar, Cmd+K, dark mode, dropdown fix |
