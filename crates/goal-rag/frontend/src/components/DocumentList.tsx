import { useState, useMemo, memo } from 'react';
import {
  FileText, Trash2, RefreshCw, FileSpreadsheet, FileCode,
  File, Calendar, Hash, HardDrive,
} from 'lucide-react';
import type { Document } from '../api/types';

interface DocumentListProps {
  documents: Document[];
  onDelete: (id: string) => Promise<void>;
  onRefresh: () => Promise<void> | void;
}

const formatFileSize = (bytes: number) => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

const formatDate = (dateString: string) => {
  const date = new Date(dateString);
  return date.toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
};

export const DocumentList = memo(function DocumentList({ documents, onDelete, onRefresh }: DocumentListProps) {
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const handleDelete = async (id: string, filename: string) => {
    if (!confirm(`Delete "${filename}" and all its chunks?`)) return;
    setDeletingId(id);
    try { await onDelete(id); } finally { setDeletingId(null); }
  };

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try { await onRefresh(); } finally { setIsRefreshing(false); }
  };

  const getFileIcon = (fileType: string) => {
    const type = fileType.toLowerCase();
    if (type === 'pdf' || type === 'docx') return <FileText className="w-5 h-5 text-red-400" />;
    if (type === 'xlsx' || type === 'csv') return <FileSpreadsheet className="w-5 h-5 text-green-400" />;
    if (type.startsWith('code') || type === 'txt' || type === 'markdown') return <FileCode className="w-5 h-5 text-blue-400" />;
    if (type === 'html') return <FileCode className="w-5 h-5 text-orange-400" />;
    return <File className="w-5 h-5 text-dark-muted" />;
  };

  const totalChunks = useMemo(() => documents.reduce((sum, d) => sum + d.total_chunks, 0), [documents]);
  const totalSize = useMemo(() => formatFileSize(documents.reduce((sum, d) => sum + d.file_size, 0)), [documents]);

  const getFileTypeBadge = (fileType: string) => {
    const type = fileType.toLowerCase();
    const colors: Record<string, string> = {
      pdf: 'bg-red-400/15 text-red-400',
      docx: 'bg-blue-400/15 text-blue-400',
      txt: 'bg-dark-surface2 text-dark-muted',
      markdown: 'bg-purple-400/15 text-purple-400',
      xlsx: 'bg-green-400/15 text-green-400',
      csv: 'bg-emerald-400/15 text-emerald-400',
      html: 'bg-orange-400/15 text-orange-400',
    };
    const colorClass = type.startsWith('code')
      ? 'bg-indigo-400/15 text-indigo-400'
      : colors[type] || 'bg-dark-surface2 text-dark-muted';
    return (
      <span className={`text-xs px-2 py-0.5 rounded ${colorClass}`}>
        {type.startsWith('code') ? type.replace('code(', '').replace(')', '') : type.toUpperCase()}
      </span>
    );
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm text-dark-muted">
            {documents.length} document{documents.length !== 1 ? 's' : ''} indexed
          </p>
        </div>
        <button
          onClick={handleRefresh}
          disabled={isRefreshing}
          className="flex items-center gap-2 px-3 py-2 text-sm text-dark-muted hover:text-dark-text hover:bg-dark-surface2 rounded-lg transition-colors"
        >
          <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
          Refresh
        </button>
      </div>

      {documents.length === 0 ? (
        <div className="bg-dark-surface rounded-lg border border-dark-border p-8 text-center">
          <FileText className="w-12 h-12 text-dark-border mx-auto mb-4" />
          <h3 className="text-lg font-medium text-dark-text mb-2">No documents yet</h3>
          <p className="text-sm text-dark-muted">
            Upload documents to start asking questions about their content.
          </p>
        </div>
      ) : (
        <div className="bg-dark-surface rounded-lg border border-dark-border divide-y divide-dark-border">
          {documents.map(doc => (
            <div key={doc.id} className="p-4 flex items-start gap-4 hover:bg-dark-surface2 transition-colors">
              <div className="flex-shrink-0 p-2 bg-dark-surface2 rounded-lg">
                {getFileIcon(doc.file_type)}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <h3 className="font-medium text-dark-text truncate">{doc.filename}</h3>
                  {getFileTypeBadge(doc.file_type)}
                </div>
                <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-dark-muted">
                  <span className="flex items-center gap-1"><Hash className="w-3 h-3" />{doc.total_chunks} chunks</span>
                  {doc.total_pages && <span className="flex items-center gap-1"><FileText className="w-3 h-3" />{doc.total_pages} pages</span>}
                  <span className="flex items-center gap-1"><HardDrive className="w-3 h-3" />{formatFileSize(doc.file_size)}</span>
                  <span className="flex items-center gap-1"><Calendar className="w-3 h-3" />{formatDate(doc.ingested_at)}</span>
                </div>
              </div>
              <button
                onClick={() => handleDelete(doc.id, doc.filename)}
                disabled={deletingId === doc.id}
                className="flex-shrink-0 p-2 text-dark-muted hover:text-red-400 hover:bg-red-400/10 rounded-lg transition-colors disabled:opacity-50"
                title="Delete document"
              >
                <Trash2 className={`w-4 h-4 ${deletingId === doc.id ? 'animate-pulse' : ''}`} />
              </button>
            </div>
          ))}
        </div>
      )}

      {documents.length > 0 && (
        <div className="grid grid-cols-3 gap-4">
          <div className="bg-dark-surface rounded-lg border border-dark-border p-4">
            <p className="text-2xl font-bold text-dark-accent">{documents.length}</p>
            <p className="text-sm text-dark-muted">Documents</p>
          </div>
          <div className="bg-dark-surface rounded-lg border border-dark-border p-4">
            <p className="text-2xl font-bold text-dark-accent">{totalChunks}</p>
            <p className="text-sm text-dark-muted">Total Chunks</p>
          </div>
          <div className="bg-dark-surface rounded-lg border border-dark-border p-4">
            <p className="text-2xl font-bold text-dark-accent">{totalSize}</p>
            <p className="text-sm text-dark-muted">Total Size</p>
          </div>
        </div>
      )}
    </div>
  );
});
