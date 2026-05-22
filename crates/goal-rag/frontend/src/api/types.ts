// API Types

export interface Document {
  id: string;
  filename: string;
  file_type: string;
  total_pages: number | null;
  total_chunks: number;
  file_size: number;
  ingested_at: string;
}

export interface Citation {
  chunk_id: string;
  document_id: string;
  filename: string;
  file_type: string;
  page_number: number | null;
  section_title: string | null;
  line_start: number | null;
  line_end: number | null;
  snippet: string;
  snippet_highlighted: string;
  similarity_score: number;
  rerank_score: number | null;
}

export interface QueryRequest {
  question: string;
  organization_id?: string;
  top_k?: number;
  similarity_threshold?: number;
  rerank?: boolean;
  document_filter?: string[];
  include_chunks?: boolean;
}

export interface QueryResponse {
  answer: string;
  citations: Citation[];
  confidence: number;
  processing_time_ms: number;
  chunks_retrieved: number;
  chunks_used: number;
}

export interface IngestResponse {
  success: boolean;
  documents: Document[];
  total_chunks_created: number;
  processing_time_ms: number;
  errors: { filename: string; error: string }[];
}

export interface DocumentListResponse {
  documents: Document[];
  total_count: number;
}

// Tool types
export interface ToolParameterSchema {
  type: string;
  description?: string;
  enum?: string[];
  default?: unknown;
  minimum?: number;
  maximum?: number;
}

export interface ToolDefinition {
  name: string;
  description: string;
  category: string;
  parameters: {
    type: string;
    properties: Record<string, ToolParameterSchema>;
    required?: string[];
  };
}

export interface ToolResult {
  success: boolean;
  data: unknown;
  summary?: string;
  row_count?: number;
  execution_ms?: number;
}

// Analytics types
export interface EmbeddingStat {
  entity_type: string;
  count: number;
}

export interface EmbeddingSearchRequest {
  query: string;
  organization_id: string;
  entity_type?: string;
  top_k?: number;
}
