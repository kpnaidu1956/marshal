# Marshal

**AI-powered management platform** with document RAG, workflow automation, timekeeping, and team analytics.

Built with Rust (Axum), React 19, PostgreSQL + pgvector, and pluggable LLM support (Ollama, OpenAI, Anthropic).

## Features

- **Ask Marshal** — AI-powered Q&A with source citations from your uploaded documents
- **Document Management** — Upload, search, and organize documents with vector embeddings
- **Workflow Automation** — Define and execute business workflows with approvals
- **Timekeeping** — Shift scheduling, time entries, pay periods, and compliance reports
- **Team Management** — Goals, tasks, assignments, and performance analytics
- **Multi-tenant** — Organization isolation with RBAC (role-based access control)
- **Self-service Trial** — User registration, org creation, EULA acceptance, join requests

## Quick Start

```bash
# 1. Clone
git clone https://github.com/your-org/marshal.git
cd marshal

# 2. Configure
cp .env.example .env
# Edit .env: set POSTGRES_PASSWORD and POSTGREST_JWT_SECRET

# 3. Start
docker-compose up -d

# 4. Pull Ollama models (first time only)
docker exec -it marshal-ollama-1 ollama pull nomic-embed-text
docker exec -it marshal-ollama-1 ollama pull phi3

# 5. Open
open http://localhost/marshal/register
```

## LLM Provider Setup

Marshal supports three LLM backends. Set `LLM_PROVIDER` in your `.env`:

### Ollama (default — free, runs locally)
```env
LLM_PROVIDER=ollama
# No API key needed. Models run on your machine.
```

### OpenAI
```env
LLM_PROVIDER=openai
OPENAI_API_KEY=sk-...
OPENAI_MODEL=gpt-4o
```

### Anthropic
```env
LLM_PROVIDER=anthropic
ANTHROPIC_API_KEY=sk-ant-...
ANTHROPIC_MODEL=claude-sonnet-4-20250514
# Anthropic doesn't provide embeddings — Ollama handles those:
EMBED_PROVIDER=ollama
```

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│   Frontend   │────▶│    Caddy      │────▶│  PostgREST   │
│  React 19    │     │  (reverse     │     │  (REST API   │
│  Tailwind    │     │   proxy)      │     │   for PG)    │
└─────────────┘     └──────┬───────┘     └──────────────┘
                           │
                    ┌──────┴───────┐
                    │              │
              ┌─────▼─────┐ ┌─────▼─────┐
              │  Goal-RAG  │ │ BPE Server │
              │  (RAG +    │ │ (Workflow  │
              │   API)     │ │  Engine)   │
              └─────┬──────┘ └─────┬─────┘
                    │              │
              ┌─────▼──────────────▼─────┐
              │      PostgreSQL          │
              │    + pgvector            │
              └──────────────────────────┘
                    │
              ┌─────▼─────┐
              │   Ollama   │
              │  (or       │
              │  OpenAI/   │
              │  Anthropic)│
              └────────────┘
```

## Configuration

All configuration is via environment variables. See [`.env.example`](.env.example) for the full reference.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `POSTGRES_PASSWORD` | Yes | — | Database password |
| `POSTGREST_JWT_SECRET` | Yes | — | JWT signing secret (min 32 chars) |
| `LLM_PROVIDER` | No | `ollama` | LLM backend: `ollama`, `openai`, `anthropic` |
| `APP_NAME` | No | `Marshal` | Platform display name |
| `APP_DOMAIN` | No | `localhost` | Your domain (for emails and CORS) |
| `SUPER_ADMIN_EMAILS` | No | — | Comma-separated admin emails |
| `RESEND_API_KEY` | No | — | Email provider API key |

## Kubernetes Deployment

K8s manifests are in `k8s/`. Update the ConfigMap and Secrets, then:

```bash
kubectl apply -f k8s/namespace.yaml
kubectl apply -f k8s/configmap.yaml
kubectl apply -f k8s/secrets.yaml
kubectl apply -f k8s/services/
kubectl apply -f k8s/deployments/
kubectl apply -f k8s/hpa/
```

## Development

```bash
# Backend
cargo build -p goal-rag --features postgres
cargo run -p goal-rag --features postgres --bin goal-rag-server

# Frontend
cd crates/marshal-ui-react
npm ci
npm run dev

# Tests
cargo test --all
```

## Tech Stack

- **Backend**: Rust, Axum 0.7, tokio, deadpool-postgres
- **Frontend**: React 19, Vite, Tailwind CSS 4, Radix UI, Zustand
- **Database**: PostgreSQL 15+ with pgvector extension
- **LLM**: Ollama (default), OpenAI, Anthropic
- **Embeddings**: nomic-embed-text (Ollama), text-embedding-3-small (OpenAI)
- **Reverse Proxy**: Caddy
- **Container Runtime**: Docker, Kubernetes

## License

[Apache License 2.0](LICENSE)
