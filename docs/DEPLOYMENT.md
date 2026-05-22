# RAG Server Deployment Instructions

## Overview

This document covers deploying the RAG server with the new filename-based document storage architecture.

---

## Prerequisites

**GCP Services Required:**
- Cloud Run (managed containers)
- Cloud Storage (GCS bucket for documents)
- Vertex AI Vector Search (embedding storage)
- Vertex AI Gemini API (LLM generation)
- Container Registry (gcr.io)
- Cloud Build (CI/CD)

**Server Requirements:**
- Rust 1.75+
- Ollama (for local embeddings in hybrid mode)
- `gcloud` CLI configured

---

## Server-Side Deployment Steps

### Step 1: Pull Latest Changes

```bash
cd /path/to/marshal
git pull origin main
```

### Step 2: Build with GCP Features

```bash
cargo build --release --features gcp -p goal-rag
```

### Step 3: Verify Configuration

Ensure `crates/goal-rag/config.toml` has correct values:
- `service_account_key_path` - path to GCP service account JSON
- `project_id` - your GCP project ID
- `gcs_bucket` - your GCS bucket name
- `vector_search_*` - Vertex AI Vector Search settings

### Step 4: Start Ollama (for hybrid mode)

```bash
# Ensure Ollama is running with embedding model
ollama pull nomic-embed-text
ollama serve
```

### Step 5: Run the Server

```bash
./target/release/goal-rag --config crates/goal-rag/config.toml
```

Or with systemd:
```bash
sudo systemctl restart goal-rag
```

### Step 6: Verify Deployment

```bash
# Health check
curl http://localhost:8080/api/info

# Check capabilities
curl http://localhost:8080/api/capabilities

# Test new upload endpoint
curl -X POST "http://localhost:8080/api/files/upload" \
  -F "file=@test.pdf" \
  -F "organization_id=test-org"
```

---

## GCP Console Setup (One-Time)

### Enable Required APIs

```bash
gcloud services enable \
  aiplatform.googleapis.com \
  storage.googleapis.com \
  run.googleapis.com \
  cloudbuild.googleapis.com
```

### Create GCS Bucket

```bash
gcloud storage buckets create gs://your-bucket \
  --location=us-central1 \
  --uniform-bucket-level-access
```

### Create Service Account

```bash
# Create service account
gcloud iam service-accounts create rag-service \
  --display-name="RAG Service Account"

# Grant permissions
gcloud projects add-iam-policy-binding your-project \
  --member="serviceAccount:your-sa@your-project.iam.gserviceaccount.com" \
  --role="roles/aiplatform.user"

gcloud projects add-iam-policy-binding your-project \
  --member="serviceAccount:your-sa@your-project.iam.gserviceaccount.com" \
  --role="roles/storage.objectAdmin"

# Download key
gcloud iam service-accounts keys create rag-key.json \
  --iam-account=your-sa@your-project.iam.gserviceaccount.com
```

---

## API Endpoints (Frontend Reference)

### New Upload Endpoint (Two-Phase Response)

```
POST /api/files/upload
```

**Request:** Multipart form
- `file` - Document file
- `organization_id` - Organization slug (required)

**Response (immediate):**
```json
{
  "success": true,
  "gcs_uploaded": true,
  "file": {
    "filename": "document.pdf",
    "organization_id": "north-county-fire",
    "gcs_path": "originals/north-county-fire/document.pdf",
    "content_hash": "sha256:abc123...",
    "file_size": 1048576,
    "action": "created"
  },
  "job_id": "uuid-for-processing",
  "processing_status_url": "/api/jobs/{job_id}"
}
```

**Action Values:**
- `created` - New file uploaded
- `replaced` - Same content, file overwritten
- `versioned` - Different content, saved as `file_v2.pdf`

### Poll Processing Status

```
GET /api/jobs/{job_id}
```

### Full API List

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/files/upload` | POST | Upload file with original filename (GCP only) |
| `/api/jobs/{id}` | GET | Get job processing progress |
| `/api/jobs/{id}/files` | GET | Get per-file progress |
| `/api/v2/query` | POST | Query with citations (frontend format) |
| `/api/files` | GET | List all tracked files |
| `/api/files/check` | POST | Check file status before upload |
| `/api/files/stats` | GET | File registry statistics |
| `/api/documents` | GET | List all documents |
| `/api/info` | GET | Server info |
| `/api/capabilities` | GET | Parser capabilities |

---

## Hybrid Mode Options

| Mode | Embeddings | Vectors | LLM | Best For |
|------|------------|---------|-----|----------|
| `full_gcp` | Vertex AI | Vertex AI | Gemini | Highest quality |
| `hybrid_vertex` | Ollama | Vertex AI | Gemini | Save embedding costs |
| `hybrid_local` | Ollama | Local HNSW | Gemini | Avoid rate limits |

Current config: `hybrid_local`

---

## Cloud Run Deployment (Alternative)

### Build Docker Image

```bash
docker build -t gcr.io/your-project/ruvector-streaming:latest \
  -f src/cloud-run/Dockerfile .

docker push gcr.io/your-project/ruvector-streaming:latest
```

### Deploy to Cloud Run

```bash
gcloud run deploy ruvector-rag \
  --image=gcr.io/your-project/ruvector-streaming:latest \
  --region=us-central1 \
  --platform=managed \
  --allow-unauthenticated \
  --memory=4Gi \
  --cpu=4 \
  --min-instances=1 \
  --max-instances=100 \
  --timeout=300s
```

### Multi-Region Deployment

```bash
gcloud builds submit --config=src/cloud-run/cloudbuild.yaml
```

Deploys to: us-central1, europe-west1, asia-east1

---

## Troubleshooting

### Check Logs

```bash
# Local
tail -f /var/log/goal-rag.log

# Cloud Run
gcloud run services logs read ruvector-rag --region=us-central1
```

### Common Issues

1. **Rate limits** - Switch to `hybrid_local` mode
2. **GCS permission denied** - Check service account roles
3. **Vertex AI errors** - Verify index endpoint is deployed
4. **Ollama connection refused** - Ensure `ollama serve` is running
