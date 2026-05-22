import { useState, useMemo, memo } from 'react';
import { FileText, ChevronDown, ChevronUp, FileCode, FileSpreadsheet } from 'lucide-react';
import type { Citation } from '../api/types';

interface CitationCardProps {
  citation: Citation;
  index: number;
}

const sanitizeHighlight = (html: string): string => {
  return html
    .replace(/<mark>/gi, '\x00MARK\x00')
    .replace(/<\/mark>/gi, '\x00/MARK\x00')
    .replace(/<[^>]*>/g, '')
    .replace(/\x00MARK\x00/g, '<mark>')
    .replace(/\x00\/MARK\x00/g, '</mark>');
};

export const CitationCard = memo(function CitationCard({ citation, index }: CitationCardProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  const getFileIcon = () => {
    const type = citation.file_type.toLowerCase();
    if (type === 'pdf' || type === 'docx') return <FileText className="w-4 h-4" />;
    if (type === 'csv' || type === 'xlsx') return <FileSpreadsheet className="w-4 h-4" />;
    if (type.startsWith('code') || type === 'txt' || type === 'markdown') return <FileCode className="w-4 h-4" />;
    return <FileText className="w-4 h-4" />;
  };

  const locationBadge = useMemo(() => {
    if (citation.page_number) return `Page ${citation.page_number}`;
    if (citation.line_start && citation.line_end) return `Lines ${citation.line_start}-${citation.line_end}`;
    return null;
  }, [citation.page_number, citation.line_start, citation.line_end]);

  const formatSnippet = (text: string, highlighted: string) => {
    if (highlighted && highlighted !== text) {
      return <span dangerouslySetInnerHTML={{ __html: sanitizeHighlight(highlighted) }} />;
    }
    return text;
  };

  const truncateSnippet = (text: string, maxLength: number = 200) => {
    if (text.length <= maxLength) return text;
    const truncated = text.slice(0, maxLength);
    const lastSpace = truncated.lastIndexOf(' ');
    return lastSpace > 0 ? truncated.slice(0, lastSpace) + '...' : truncated + '...';
  };

  return (
    <div className="bg-dark-surface border border-dark-border rounded-lg overflow-hidden hover:border-dark-muted/30 transition-colors">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-dark-surface2 transition-colors"
      >
        <span className="flex-shrink-0 w-6 h-6 bg-dark-accent/20 text-dark-accent rounded-full flex items-center justify-center text-xs font-medium">
          {index}
        </span>
        <span className="flex-shrink-0 text-dark-muted">{getFileIcon()}</span>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-medium text-dark-text truncate">{citation.filename}</span>
            {locationBadge && (
              <span className="flex-shrink-0 text-xs bg-dark-surface2 text-dark-muted px-2 py-0.5 rounded">
                {locationBadge}
              </span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2 text-xs text-dark-muted">
          <span
            className={`px-2 py-0.5 rounded ${
              citation.similarity_score >= 0.7
                ? 'bg-green-400/15 text-green-400'
                : citation.similarity_score >= 0.5
                ? 'bg-yellow-400/15 text-yellow-400'
                : 'bg-dark-surface2 text-dark-muted'
            }`}
          >
            {Math.round(citation.similarity_score * 100)}% match
          </span>
          {isExpanded ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
        </div>
      </button>

      {!isExpanded && (
        <div className="px-3 pb-2">
          <p className="text-sm text-dark-muted line-clamp-2">
            {truncateSnippet(citation.snippet)}
          </p>
        </div>
      )}

      {isExpanded && (
        <div className="border-t border-dark-border">
          <div className="px-3 py-3">
            <p className="text-sm font-medium text-dark-muted mb-2">Source excerpt:</p>
            <div className="bg-dark-surface2 rounded-lg p-3 text-sm text-dark-text leading-relaxed">
              {formatSnippet(citation.snippet, citation.snippet_highlighted)}
            </div>
          </div>
          <div className="px-3 pb-3 flex flex-wrap gap-2 text-xs">
            {citation.section_title && (
              <span className="bg-blue-400/15 text-blue-400 px-2 py-1 rounded">
                Section: {citation.section_title}
              </span>
            )}
            <span className="bg-dark-surface2 text-dark-muted px-2 py-1 rounded">
              Type: {citation.file_type}
            </span>
            {citation.rerank_score && (
              <span className="bg-purple-400/15 text-purple-400 px-2 py-1 rounded">
                Rerank: {Math.round(citation.rerank_score * 100)}%
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
});
