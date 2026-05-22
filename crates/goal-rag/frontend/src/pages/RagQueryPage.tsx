import { useState, useCallback } from 'react';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';

export function RagQueryPage() {
  const { orgId, showResult } = useApp();
  const [question, setQuestion] = useState('');
  const [topK, setTopK] = useState(5);
  const [searchText, setSearchText] = useState('');
  const [searchLimit, setSearchLimit] = useState(10);
  const [loading, setLoading] = useState<string | null>(null);

  const executeQuery = useCallback(async (version: 'v1' | 'v2') => {
    if (!question.trim()) return;
    setLoading(version);
    try {
      const start = performance.now();
      const body = { question, organization_id: orgId, top_k: topK };
      const data = version === 'v2' ? await api.queryV2(body) : await api.query(body);
      const elapsed = Math.round(performance.now() - start);
      showResult(data, `RAG ${version} query`, true, `${elapsed}ms`);
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [question, orgId, topK, showResult]);

  const executeStringSearch = useCallback(async () => {
    if (!searchText.trim()) return;
    setLoading('string');
    try {
      const start = performance.now();
      const data = await api.stringSearch({ query: searchText, organization_id: orgId, limit: searchLimit });
      const elapsed = Math.round(performance.now() - start);
      showResult(data, 'String search', true, `${elapsed}ms`);
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [searchText, orgId, searchLimit, showResult]);

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">RAG Query</h2>
      <p className="text-sm text-dark-muted mb-4">Query documents using embeddings or literal text search</p>

      {/* v2 Query */}
      <div className="bg-dark-surface border border-dark-border rounded-lg p-4 mb-4">
        <h3 className="text-sm font-semibold mb-3">v2 Query (Recommended)</h3>
        <div className="mb-3">
          <label className="block text-xs text-dark-muted mb-1">Question</label>
          <textarea
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            placeholder="Ask a question about your documents..."
            className="w-full min-h-[80px] bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm resize-vertical focus:outline-none focus:border-dark-accent"
          />
        </div>
        <div className="flex items-end gap-3">
          <div>
            <label className="block text-xs text-dark-muted mb-1">Top K</label>
            <input
              type="number"
              value={topK}
              onChange={(e) => setTopK(parseInt(e.target.value) || 5)}
              min={1}
              max={50}
              className="w-24 bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
            />
          </div>
          <button
            onClick={() => executeQuery('v2')}
            disabled={loading !== null}
            className="px-4 py-2 bg-dark-accent text-white rounded-md text-sm font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === 'v2' ? <><span className="spinner mr-2" />Querying...</> : 'Query v2'}
          </button>
          <button
            onClick={() => executeQuery('v1')}
            disabled={loading !== null}
            className="px-4 py-2 bg-dark-surface2 text-dark-text rounded-md text-sm hover:bg-dark-border disabled:opacity-50 transition-colors"
          >
            {loading === 'v1' ? 'Querying...' : 'Query v1'}
          </button>
        </div>
      </div>

      {/* String Search */}
      <div className="bg-dark-surface border border-dark-border rounded-lg p-4">
        <h3 className="text-sm font-semibold mb-3">String Search</h3>
        <div className="mb-3">
          <label className="block text-xs text-dark-muted mb-1">Search Text</label>
          <input
            type="text"
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            placeholder="Exact text to find..."
            className="w-full bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
          />
        </div>
        <div className="flex items-end gap-3">
          <div>
            <label className="block text-xs text-dark-muted mb-1">Limit</label>
            <input
              type="number"
              value={searchLimit}
              onChange={(e) => setSearchLimit(parseInt(e.target.value) || 10)}
              min={1}
              max={100}
              className="w-24 bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
            />
          </div>
          <button
            onClick={executeStringSearch}
            disabled={loading !== null}
            className="px-4 py-2 bg-dark-accent text-white rounded-md text-sm font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === 'string' ? <><span className="spinner mr-2" />Searching...</> : 'Search'}
          </button>
        </div>
      </div>
    </div>
  );
}
