import { useState, useCallback } from 'react';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';
import type { EmbeddingStat } from '../api/types';

export function AnalyticsPage() {
  const { orgId, showResult } = useApp();
  const [stats, setStats] = useState<EmbeddingStat[] | null>(null);
  const [query, setQuery] = useState('');
  const [entityType, setEntityType] = useState('');
  const [topK, setTopK] = useState(10);
  const [loading, setLoading] = useState<string | null>(null);

  const loadStats = useCallback(async () => {
    setLoading('stats');
    try {
      const data = await api.getEmbeddingStats(orgId);
      setStats(data);
      showResult(data, 'Embedding stats', true, '');
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [orgId, showResult]);

  const searchEmbeddings = useCallback(async () => {
    if (!query.trim()) return;
    setLoading('search');
    try {
      const start = performance.now();
      const body = { query, organization_id: orgId, top_k: topK, ...(entityType && { entity_type: entityType }) };
      const data = await api.searchEmbeddings(body);
      const elapsed = Math.round(performance.now() - start);
      showResult(data, 'Embedding search', true, `${elapsed}ms`);
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [query, orgId, topK, entityType, showResult]);

  const triggerBackfill = useCallback(async () => {
    setLoading('backfill');
    try {
      const data = await api.backfillEmbeddings(orgId);
      showResult(data, 'Backfill triggered', true, '');
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [orgId, showResult]);

  const triggerSentiment = useCallback(async () => {
    setLoading('sentiment');
    try {
      const data = await api.backfillSentiment(orgId);
      showResult(data, 'Sentiment backfill triggered', true, '');
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [orgId, showResult]);

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">Analytics & Embeddings</h2>
      <p className="text-sm text-dark-muted mb-4">Entity embedding intelligence</p>

      {/* Stats */}
      <div className="bg-dark-surface border border-dark-border rounded-lg p-4 mb-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-semibold">Embedding Stats</h3>
          <button
            onClick={loadStats}
            disabled={loading !== null}
            className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === 'stats' ? 'Loading...' : 'Load Stats'}
          </button>
        </div>
        {stats && (
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3">
            {stats.map((s) => (
              <div key={s.entity_type} className="bg-dark-surface2 rounded-lg p-3 text-center">
                <div className="text-xl font-bold text-dark-accent">{s.count}</div>
                <div className="text-xs text-dark-muted mt-1">{s.entity_type}</div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Semantic Search */}
      <div className="bg-dark-surface border border-dark-border rounded-lg p-4 mb-4">
        <h3 className="text-sm font-semibold mb-3">Semantic Search (Entities)</h3>
        <div className="mb-3">
          <label className="block text-xs text-dark-muted mb-1">Query</label>
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Natural language search..."
            className="w-full bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
          />
        </div>
        <div className="flex items-end gap-3">
          <div>
            <label className="block text-xs text-dark-muted mb-1">Entity Type</label>
            <select
              value={entityType}
              onChange={(e) => setEntityType(e.target.value)}
              className="w-40 bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
            >
              <option value="">All</option>
              <option value="task">task</option>
              <option value="goal">goal</option>
              <option value="task_comment">task_comment</option>
              <option value="message">message</option>
              <option value="user">user</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-dark-muted mb-1">Top K</label>
            <input
              type="number"
              value={topK}
              onChange={(e) => setTopK(parseInt(e.target.value) || 10)}
              min={1}
              max={50}
              className="w-20 bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
            />
          </div>
          <button
            onClick={searchEmbeddings}
            disabled={loading !== null}
            className="px-4 py-2 bg-dark-accent text-white rounded-md text-sm font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === 'search' ? <><span className="spinner mr-2" />Searching...</> : 'Search'}
          </button>
        </div>
      </div>

      {/* Actions */}
      <div className="bg-dark-surface border border-dark-border rounded-lg p-4">
        <h3 className="text-sm font-semibold mb-3">Actions</h3>
        <div className="flex gap-2 flex-wrap">
          <button
            onClick={triggerBackfill}
            disabled={loading !== null}
            className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === 'backfill' ? 'Running...' : 'Backfill Embeddings'}
          </button>
          <button
            onClick={triggerSentiment}
            disabled={loading !== null}
            className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === 'sentiment' ? 'Running...' : 'Backfill Sentiment'}
          </button>
        </div>
      </div>
    </div>
  );
}
