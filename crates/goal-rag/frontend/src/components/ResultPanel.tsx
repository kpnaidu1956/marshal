import { useState, useCallback, memo } from 'react';
import { ChevronDown, ChevronUp, Copy, Check } from 'lucide-react';
import { JsonViewer } from './JsonViewer';
import { useApp } from '../context/AppContext';

export const ResultPanel = memo(function ResultPanel() {
  const { result } = useApp();
  const [expanded, setExpanded] = useState(true);
  const [copied, setCopied] = useState(false);

  const copyResult = useCallback(() => {
    if (result?.data) {
      navigator.clipboard.writeText(JSON.stringify(result.data, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }, [result]);

  if (!result) return null;

  return (
    <div className="border-t border-dark-border flex-shrink-0">
      {/* Status bar */}
      <div className="flex items-center gap-3 px-4 py-2 bg-dark-surface text-xs">
        <span className={`px-2 py-0.5 rounded font-semibold ${
          result.success
            ? 'bg-green-400/15 text-green-400'
            : 'bg-red-400/15 text-red-400'
        }`}>
          {result.success ? 'SUCCESS' : 'ERROR'}
        </span>
        <span className="text-dark-muted">{result.summary}</span>
        <span className="text-dark-muted">{result.meta}</span>
        <div className="ml-auto flex items-center gap-2">
          <button
            onClick={copyResult}
            className="flex items-center gap-1 px-2 py-1 rounded bg-dark-surface2 text-dark-muted hover:text-dark-text transition-colors"
          >
            {copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
            {copied ? 'Copied' : 'Copy'}
          </button>
          <button
            onClick={() => setExpanded(!expanded)}
            className="flex items-center gap-1 px-2 py-1 rounded bg-dark-surface2 text-dark-muted hover:text-dark-text transition-colors"
          >
            {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronUp className="w-3 h-3" />}
            {expanded ? 'Collapse' : 'Expand'}
          </button>
        </div>
      </div>

      {/* JSON viewer */}
      {expanded && (
        <div className="max-h-[40vh] overflow-y-auto bg-dark-surface px-4 py-3">
          <JsonViewer data={result.data} />
        </div>
      )}
    </div>
  );
});
