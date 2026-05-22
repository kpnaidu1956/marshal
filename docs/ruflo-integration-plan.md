# Ruflo Integration Plan for marshal

## Executive Summary

Integrate Ruflo's AI agent orchestration (TypeScript/Node.js) into marshal's Rust/Axum + React platform to enhance BPE workflow execution with intelligent agents, augment RAG with HNSW vector search, and add multi-provider LLM support.

## Architecture: Sidecar Model

Ruflo runs as a **sidecar service** alongside goal-rag (8080) and bpe-server (8090), communicating via:
- **MCP protocol** (Ruflo's 310+ tools exposed as MCP server)
- **HTTP API** (Ruflo endpoints called from Rust via reqwest)
- **Shared PostgreSQL** (Ruflo's AgentDB configured to use goalrag DB)

```
┌─────────────────────────────────────────────────┐
│                   nginx (443)                     │
├──────────┬──────────┬──────────┬─────────────────┤
│ goal-rag │ bpe-srv  │ PostgREST│  ruflo-sidecar  │
│  :8080   │  :8090   │  :3000   │     :8100       │
├──────────┴──────────┴──────────┴─────────────────┤
│              PostgreSQL (goalrag)                  │
│         pgvector + AgentDB tables                 │
└───────────────────────────────────────────────────┘
```

## Phase 1: Agent-Powered BPE Workflows (High Impact)

### What
BPE "automated" and "llm_action" step types currently have no execution backend. Wire them to Ruflo agents.

### How
1. **New BPE integration type: `ruflo_agent`** — step_templates with `integration_type: "ruflo_agent"` include `integration_config: { agent_type, prompt, tools[] }`
2. **bpe-server executor** — when executing a ruflo_agent step, POST to `http://localhost:8100/api/agent/spawn` with the agent config
3. **Ruflo agent types to expose**:
   - `researcher` — gather information, summarize documents
   - `coder` — generate/review code artifacts
   - `reviewer` — validate step outputs, compliance checks
   - `planner` — decompose complex approval chains
4. **Callback mechanism** — Ruflo agent POSTs result back to `bpe-server /api/v1/{org}/executions/{id}/steps/{step}/complete`

### Files to Change
- `crates/bpe-core/src/models.rs` — add `RufloAgent` to IntegrationType enum
- `crates/bpe-server/src/execution.rs` — add ruflo_agent executor branch
- `crates/marshal-ui-react/src/components/bpe/WorkflowStepCard.tsx` — agent type picker for ruflo_agent integration
- New: `crates/bpe-server/src/integrations/ruflo.rs` — HTTP client for Ruflo sidecar

### Effort: ~3 days

## Phase 2: HNSW Vector Memory for RAG (Medium Impact)

### What
Ruflo's AgentDB uses HNSW indexing (150x-12,500x faster than brute-force). Augment goal-rag's pgvector search with a fast HNSW cache layer for frequently-queried embeddings.

### How
1. **Hybrid search**: goal-rag queries Ruflo's HNSW index first (sub-millisecond), falls back to pgvector for cold queries
2. **Warm cache**: on document ingest, goal-rag sends embedding to both pgvector AND Ruflo HNSW via MCP `memory_store`
3. **Cross-session memory**: Ruflo's persistent memory stores conversation context, enabling RAG queries to consider prior interactions

### Files to Change
- `crates/goal-rag/src/embeddings.rs` — dual-write to pgvector + Ruflo HNSW
- `crates/goal-rag/src/query.rs` — hybrid search: HNSW first, pgvector fallback
- New: `crates/goal-rag/src/hnsw_bridge.rs` — MCP client to Ruflo memory tools

### Effort: ~2 days

## Phase 3: Multi-Provider LLM (Medium Impact)

### What
goal-rag currently uses ollama/nomic-embed-text for embeddings. Ruflo supports Anthropic, OpenAI, Google, Cohere, Ollama. Route LLM calls through Ruflo for provider flexibility.

### How
1. **LLM router endpoint** on Ruflo: `POST /api/llm/complete` with provider selection
2. **BPE LLM Action steps** use Ruflo's multi-provider support instead of direct API calls
3. **Embedding provider switching** — configure preferred embedding model per organization
4. **Fallback chains** — if primary provider is down, Ruflo auto-falls back to secondary

### Files to Change
- `crates/bpe-server/src/integrations/ruflo.rs` — add LLM routing methods
- `crates/goal-rag/src/embeddings.rs` — optional Ruflo LLM routing for non-ollama providers

### Effort: ~2 days

## Phase 4: Swarm Coordination for Complex Workflows (Lower Priority)

### What
Multi-step BPE workflows with parallel branches could benefit from Ruflo's swarm topologies (mesh, hierarchical, ring) for coordinated agent execution.

### How
1. **Swarm-backed workflow execution** — when a BPE workflow has 3+ parallel automated steps, spawn a Ruflo swarm instead of individual agents
2. **Topology auto-selection** — Ruflo picks optimal topology based on step graph structure
3. **Consensus for approvals** — use Ruflo's Byzantine/Raft consensus for multi-approver decisions

### Files to Change
- `crates/bpe-server/src/execution.rs` — swarm detection logic for parallel step groups
- New: `crates/bpe-server/src/integrations/ruflo_swarm.rs` — swarm lifecycle management

### Effort: ~3 days

## Phase 5: React Frontend Enhancements (Lower Priority)

### What
Surface Ruflo capabilities in the marshal-ui-react frontend.

### How
1. **Agent activity panel** — real-time view of Ruflo agents working on BPE steps (WebSocket from Ruflo)
2. **Swarm visualization** — show agent topology graph for active workflow executions
3. **LLM provider settings page** — configure per-org LLM provider preferences
4. **Memory search UI** — search Ruflo's HNSW memory from the frontend

### New Files
- `src/pages/bpe/BpeAgentsPage.tsx` — agent monitoring dashboard
- `src/components/bpe/AgentActivityPanel.tsx` — real-time agent status
- `src/pages/settings/LlmProvidersPage.tsx` — provider configuration

### Effort: ~3 days

## Deployment

### Ruflo Sidecar Setup
```bash
# On app-server
cd /home/deploy
git clone <ruflo-repo> ruflo-sidecar
cd ruflo-sidecar
npm install
# Configure to use existing PostgreSQL
echo 'DATABASE_URL=postgres://localhost/goalrag' >> .env
echo 'PORT=8100' >> .env
echo 'MCP_PORT=8101' >> .env

# Systemd service
sudo cat > /etc/systemd/system/ruflo.service << 'EOF'
[Unit]
Description=Ruflo AI Agent Sidecar
After=postgresql.service

[Service]
Type=simple
User=deploy
WorkingDirectory=/home/deploy/ruflo-sidecar
ExecStart=/usr/bin/node dist/index.js
Restart=always
Environment=NODE_ENV=production

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl enable ruflo && sudo systemctl start ruflo
```

### Nginx Addition
```nginx
location /ruflo/ {
    proxy_pass http://127.0.0.1:8100/;
    proxy_set_header X-Api-Key $http_x_api_key;
}
```

## Priority Order

| Phase | Impact | Effort | Priority |
|-------|--------|--------|----------|
| 1. Agent-Powered BPE | High | 3d | P0 |
| 2. HNSW Vector Memory | Medium | 2d | P1 |
| 3. Multi-Provider LLM | Medium | 2d | P1 |
| 4. Swarm Coordination | Lower | 3d | P2 |
| 5. Frontend Enhancements | Lower | 3d | P2 |

**Total estimated effort: ~13 days**

## Risks & Mitigations

- **Node.js dependency**: Ruflo is TypeScript; the rest of the stack is Rust. Mitigation: sidecar model keeps them loosely coupled. If Ruflo is down, BPE falls back to manual-only steps.
- **Memory overhead**: Ruflo + Node.js adds ~200-400MB RAM. Mitigation: app-server has sufficient headroom; HNSW index is memory-mapped.
- **Schema conflicts**: Ruflo's AgentDB tables could conflict with existing `goalrag` schema. Mitigation: Ruflo tables go in a dedicated `ruflo` schema, not `api` or `bpe`.
- **Auth bridging**: Ruflo needs to validate the same JWTs. Mitigation: share `POSTGREST_JWT_SECRET` env var.
