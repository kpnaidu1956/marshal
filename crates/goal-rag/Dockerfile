# Multi-stage Dockerfile for goal-rag RAG service
# Optimized for GCP Cloud Run deployment

# ==============================================================================
# Stage 1: Build the Rust binary
# ==============================================================================
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ruvector-core ./crates/ruvector-core
COPY crates/goal-rag ./crates/goal-rag

# Build release binary with GCP features
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN cargo build --release --package goal-rag --features gcp --bin goal-rag-server

# ==============================================================================
# Stage 2: Runtime image with document extraction tools
# ==============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies and document extraction tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    # SSL/TLS
    ca-certificates \
    libssl3 \
    # PDF extraction
    poppler-utils \
    # OCR for scanned documents
    tesseract-ocr \
    tesseract-ocr-eng \
    # Document conversion (DOCX, RTF, EPUB, ODT)
    pandoc \
    # Legacy Office formats (DOC, PPT, XLS)
    libreoffice-writer-nogui \
    libreoffice-calc-nogui \
    libreoffice-impress-nogui \
    # Image processing for OCR
    imagemagick \
    # Utilities
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for security
RUN groupadd -r raguser && useradd -r -g raguser raguser

# Create application directories
RUN mkdir -p /app/data /app/config /app/logs \
    && chown -R raguser:raguser /app

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/goal-rag-server /app/goal-rag-server

# Copy default config (if exists)
COPY --chown=raguser:raguser crates/goal-rag/config.toml /app/config/config.toml

# Set ownership
RUN chown -R raguser:raguser /app

# Switch to non-root user
USER raguser

# Environment variables
ENV RUST_LOG=info,goal_rag=debug \
    RAG_CONFIG=/app/config/config.toml \
    RAG_DATA_DIR=/app/data \
    RAG_HOST=0.0.0.0 \
    RAG_PORT=8080 \
    # GCP configuration (set via Cloud Run env vars)
    GOOGLE_APPLICATION_CREDENTIALS="" \
    GCS_BUCKET="" \
    GCP_PROJECT_ID="" \
    # Ollama/LLM configuration
    OLLAMA_HOST=http://localhost:11434 \
    # Production controls
    RATE_LIMIT_ENABLED=true \
    MAX_UPLOAD_SIZE=104857600

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose port
EXPOSE 8080

# Run the server
ENTRYPOINT ["/app/goal-rag-server"]
