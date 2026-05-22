# Contributing to Marshal

Thank you for your interest in contributing to Marshal!

## Development Setup

1. **Prerequisites**: Rust 1.88+, Node.js 20+, PostgreSQL 15+ with pgvector, Ollama
2. **Clone**: `git clone https://github.com/your-org/marshal.git`
3. **Backend**: `cargo build -p goal-rag --features postgres`
4. **Frontend**: `cd crates/marshal-ui-react && npm ci && npm run dev`
5. **Database**: Run `scripts/trial/001-bootstrap-schema.sql` against your PostgreSQL

Or use Docker: `cp .env.example .env && docker-compose up`

## Code Style

- **Rust**: `cargo fmt` and `cargo clippy --all-targets`
- **TypeScript**: Standard React/TypeScript conventions
- **Commits**: Conventional commits (`feat:`, `fix:`, `docs:`, etc.)

## Pull Requests

1. Fork the repository
2. Create a feature branch from `main`
3. Write tests for new functionality
4. Ensure `cargo test --all` and `npm run build` pass
5. Submit a PR with a clear description

## Architecture

- `crates/goal-rag` — RAG backend (Axum, PostgreSQL, pgvector)
- `crates/bpe-core` + `crates/bpe-server` — Business Process Engine
- `crates/marshal-ui-react` — React 19 frontend (Vite, Tailwind, Radix UI)
- `docker/` — Dockerfiles and Caddy configs
- `k8s/` — Kubernetes manifests
- `scripts/` — Database schema and seed scripts

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
