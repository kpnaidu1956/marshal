// API Client for Goal-RAG Server

import type {
  QueryRequest,
  QueryResponse,
  IngestResponse,
  DocumentListResponse,
  Document,
  ToolDefinition,
  ToolResult,
  EmbeddingStat,
  EmbeddingSearchRequest,
} from './types';

class ApiClient {
  private baseUrl: string = '';

  setBaseUrl(url: string) {
    this.baseUrl = url.replace(/\/+$/, '');
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }

  private async fetch<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
    });

    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: 'Unknown error' }));
      throw new Error(error.error?.message || error.message || `HTTP ${response.status}`);
    }

    return response.json();
  }

  private async rawFetch(endpoint: string, options?: RequestInit): Promise<{ ok: boolean; status: number; data: unknown }> {
    const url = `${this.baseUrl}${endpoint}`;
    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
    });
    const data = await response.json().catch(() => null);
    return { ok: response.ok, status: response.status, data };
  }

  // ── Tools ──────────────────────────────────────────────────────────────

  async getToolManifest(): Promise<ToolDefinition[]> {
    return this.fetch<ToolDefinition[]>('/api/tools/manifest');
  }

  async executeTool(tool: string, params: Record<string, unknown>): Promise<ToolResult> {
    return this.fetch<ToolResult>('/api/tools/execute', {
      method: 'POST',
      body: JSON.stringify({ tool, params }),
    });
  }

  // ── RAG Query ──────────────────────────────────────────────────────────

  async query(request: QueryRequest): Promise<QueryResponse> {
    return this.fetch<QueryResponse>('/api/query', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async queryV2(request: QueryRequest): Promise<unknown> {
    const { data } = await this.rawFetch('/api/v2/query', {
      method: 'POST',
      body: JSON.stringify(request),
    });
    return data;
  }

  async stringSearch(request: { query: string; organization_id: string; limit?: number }): Promise<unknown> {
    const { data } = await this.rawFetch('/api/string-search', {
      method: 'POST',
      body: JSON.stringify(request),
    });
    return data;
  }

  // ── Documents ──────────────────────────────────────────────────────────

  async listDocuments(orgId?: string): Promise<DocumentListResponse> {
    const qs = orgId ? `?organization_id=${orgId}` : '';
    return this.fetch<DocumentListResponse>(`/api/documents${qs}`);
  }

  async getDocument(id: string): Promise<Document> {
    return this.fetch<Document>(`/api/documents/${id}`);
  }

  async deleteDocument(id: string): Promise<{ success: boolean; deleted_chunks: number }> {
    return this.fetch(`/api/documents/${id}`, { method: 'DELETE' });
  }

  // ── Files ──────────────────────────────────────────────────────────────

  async ingest(files: File[], orgId?: string): Promise<IngestResponse> {
    const formData = new FormData();
    for (const file of files) {
      formData.append('files', file);
    }
    if (orgId) formData.append('organization_id', orgId);

    const response = await fetch(`${this.baseUrl}/api/ingest`, {
      method: 'POST',
      body: formData,
    });

    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: 'Upload failed' }));
      throw new Error(error.error?.message || error.message || `HTTP ${response.status}`);
    }

    return response.json();
  }

  async listFiles(orgId: string): Promise<unknown> {
    const { data } = await this.rawFetch(`/api/files?organization_id=${orgId}`);
    return data;
  }

  async getFileStats(orgId: string): Promise<unknown> {
    const { data } = await this.rawFetch(`/api/files/stats?organization_id=${orgId}`);
    return data;
  }

  async listFailedFiles(orgId: string): Promise<unknown> {
    const { data } = await this.rawFetch(`/api/files/failed?organization_id=${orgId}`);
    return data;
  }

  // ── Analytics ──────────────────────────────────────────────────────────

  async getEmbeddingStats(orgId: string): Promise<EmbeddingStat[]> {
    return this.fetch<EmbeddingStat[]>(`/api/analytics/embeddings/stats?organization_id=${orgId}`);
  }

  async searchEmbeddings(request: EmbeddingSearchRequest): Promise<unknown> {
    const { data } = await this.rawFetch('/api/analytics/embeddings/search', {
      method: 'POST',
      body: JSON.stringify(request),
    });
    return data;
  }

  async backfillEmbeddings(orgId: string): Promise<unknown> {
    const { data } = await this.rawFetch('/api/analytics/embeddings/backfill', {
      method: 'POST',
      body: JSON.stringify({ organization_id: orgId }),
    });
    return data;
  }

  async backfillSentiment(orgId: string): Promise<unknown> {
    const { data } = await this.rawFetch('/api/analytics/embeddings/backfill-sentiment', {
      method: 'POST',
      body: JSON.stringify({ organization_id: orgId }),
    });
    return data;
  }

  // ── System ─────────────────────────────────────────────────────────────

  async healthCheck(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  async getInfo(): Promise<unknown> {
    const { data } = await this.rawFetch('/api/info');
    return data;
  }

  async getCapabilities(): Promise<unknown> {
    const { data } = await this.rawFetch('/api/capabilities');
    return data;
  }

  async getParsersStatus(): Promise<unknown> {
    const { data } = await this.rawFetch('/api/system/parsers');
    return data;
  }
}

export const api = new ApiClient();
