import { statusToError } from './error'

// ---------------------------------------------------------------------------
// RAG Client — talks to the goal-rag service (/api/* endpoints)
// ---------------------------------------------------------------------------

export interface RagDocument {
 id: string
 filename: string
 file_type: string
 total_pages: number | null
 total_chunks: number
 file_size: number
 ingested_at: string
 archived: boolean
}

export interface RagDocumentsResponse {
 documents: RagDocument[]
}

export interface QueryRequest {
 question: string
 organization_id: string
 top_k?: number
}

export interface QueryResponse {
 answer: string
 citations: Citation[]
 query_type?: unknown
}

export interface Citation {
 index: number
 source: CitationSource
 snippet: CitationSnippet
 relevance: CitationRelevance
}

export interface CitationSource {
 document_id: string
 chunk_id: string
 filename: string
 file_type: string
 page: number | null
}

export interface CitationSnippet {
 text: string
 preview: string
}

export interface CitationRelevance {
 score: number
 label: string
}

export interface ToolDefinition {
 name: string
 description: string
 parameters: Record<string, unknown>
}

export interface ToolCall {
 tool: string
 params: Record<string, unknown>
}

export interface ToolResult {
 tool: string
 result: unknown
 error: string | null
}

export interface EmbeddingStat {
 entity_type: string
 count: number
}

export class RagClient {
 constructor(
 public baseUrl: string,
 public apiKey: string,
 public token: string | null,
 ) {}

 private headers(): Record<string, string> {
 const h: Record<string, string> = { Accept: 'application/json' }
 if (this.token) h['Authorization'] = `Bearer ${this.token}`
 if (this.apiKey) h['apikey'] = this.apiKey
 return h
 }

 private async handleResponse<T>(resp: Response): Promise<T> {
 if (!resp.ok) {
 if (resp.status === 401) {
 const { useAuthStore } = await import('@/stores/auth')
 useAuthStore.getState().logout()
 window.location.href = '/login'
 throw new Error('Session expired. Please log in again.')
 }
 const body = await resp.text()
 throw statusToError(resp.status, body)
 }
 return resp.json() as Promise<T>
 }

 /** Generic GET to any path. */
 async get<T>(path: string): Promise<T> {
 const resp = await fetch(`${this.baseUrl}${path}`, { headers: this.headers() })
 return this.handleResponse<T>(resp)
 }

 /** GET /health */
 async healthCheck(): Promise<boolean> {
 try {
 const resp = await fetch(`${this.baseUrl}/health`)
 return resp.status === 200
 } catch {
 return false
 }
 }

 /** GET /api/tools/manifest */
 async getToolManifest(): Promise<ToolDefinition[]> {
 return this.get<ToolDefinition[]>('/api/tools/manifest')
 }

 /** POST /api/tools/execute */
 async executeTool(tool: string, params: Record<string, unknown>): Promise<ToolResult> {
 const resp = await fetch(`${this.baseUrl}/api/tools/execute`, {
 method: 'POST',
 headers: { ...this.headers(), 'Content-Type': 'application/json' },
 body: JSON.stringify({ tool, params }),
 })
 return this.handleResponse<ToolResult>(resp)
 }

 /** POST /api/tools/batch */
 async batchExecute(calls: ToolCall[]): Promise<ToolResult[]> {
 const resp = await fetch(`${this.baseUrl}/api/tools/batch`, {
 method: 'POST',
 headers: { ...this.headers(), 'Content-Type': 'application/json' },
 body: JSON.stringify({ calls }),
 })
 return this.handleResponse<ToolResult[]>(resp)
 }

 /** POST /api/v2/query — ask a question against the document corpus. */
 async queryV2(request: QueryRequest): Promise<QueryResponse> {
 const resp = await fetch(`${this.baseUrl}/api/v2/query`, {
 method: 'POST',
 headers: { ...this.headers(), 'Content-Type': 'application/json' },
 body: JSON.stringify(request),
 })
 return this.handleResponse<QueryResponse>(resp)
 }

 /** POST /api/string-search */
 async stringSearch(query: string, orgId: string, limit?: number): Promise<unknown> {
 const resp = await fetch(`${this.baseUrl}/api/string-search`, {
 method: 'POST',
 headers: { ...this.headers(), 'Content-Type': 'application/json' },
 body: JSON.stringify({ query, organization_id: orgId, limit }),
 })
 return this.handleResponse<unknown>(resp)
 }

 /** GET /api/documents?organization_id={slug} */
 async listDocuments(orgSlug: string): Promise<RagDocumentsResponse> {
 return this.get<RagDocumentsResponse>(`/api/documents?organization_id=${orgSlug}`)
 }

 /** POST /api/documents/{id}/archive */
 async archiveDocument(id: string, orgSlug: string): Promise<unknown> {
 const resp = await fetch(
 `${this.baseUrl}/api/documents/${id}/archive?organization_id=${orgSlug}`,
 { method: 'POST', headers: this.headers() },
 )
 return this.handleResponse<unknown>(resp)
 }

 /** POST /api/documents/{id}/unarchive */
 async unarchiveDocument(id: string, orgSlug: string): Promise<unknown> {
 const resp = await fetch(
 `${this.baseUrl}/api/documents/${id}/unarchive?organization_id=${orgSlug}`,
 { method: 'POST', headers: this.headers() },
 )
 return this.handleResponse<unknown>(resp)
 }

 /** DELETE /api/documents/{id} */
 async deleteDocument(id: string, orgSlug: string): Promise<unknown> {
 const resp = await fetch(
 `${this.baseUrl}/api/documents/${id}?organization_id=${orgSlug}`,
 { method: 'DELETE', headers: this.headers() },
 )
 return this.handleResponse<unknown>(resp)
 }

 /** GET /api/analytics/embeddings/stats?organization_id={id} */
 async getEmbeddingStats(orgId: string): Promise<EmbeddingStat[]> {
 const raw = await this.get<Record<string, unknown>>(
 `/api/analytics/embeddings/stats?organization_id=${orgId}`,
 )
 if (Array.isArray(raw)) return raw as EmbeddingStat[]
 if (raw && typeof raw === 'object' && 'by_type' in raw && Array.isArray(raw.by_type)) {
 return raw.by_type as EmbeddingStat[]
 }
 return []
 }
}
