import { useState, useCallback } from 'react';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';
import { JsonViewer } from '../components/JsonViewer';

interface FileRecord {
  filename?: string;
  name?: string;
  status?: string;
  chunks?: number;
}

export function FilesPage() {
  const { orgId, showResult } = useApp();
  const [files, setFiles] = useState<FileRecord[] | null>(null);
  const [display, setDisplay] = useState<{ type: string; data: unknown } | null>(null);
  const [loading, setLoading] = useState<string | null>(null);

  const loadFiles = useCallback(async () => {
    setLoading('files');
    try {
      const data = await api.listFiles(orgId);
      const arr = Array.isArray(data) ? data : [];
      setFiles(arr);
      setDisplay({ type: 'files', data });
      showResult(data, 'Files loaded', true, `${arr.length} files`);
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [orgId, showResult]);

  const loadStats = useCallback(async () => {
    setLoading('stats');
    try {
      const data = await api.getFileStats(orgId);
      setDisplay({ type: 'stats', data });
      showResult(data, 'File stats', true, '');
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [orgId, showResult]);

  const loadFailed = useCallback(async () => {
    setLoading('failed');
    try {
      const data = await api.listFailedFiles(orgId);
      setDisplay({ type: 'failed', data });
      showResult(data, 'Failed files', true, '');
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [orgId, showResult]);

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">File Manager</h2>
      <p className="text-sm text-dark-muted mb-4">Track uploaded files and ingestion status</p>

      <div className="flex gap-2 mb-4">
        <button onClick={loadFiles} disabled={loading !== null}
          className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors">
          {loading === 'files' ? 'Loading...' : 'List Files'}
        </button>
        <button onClick={loadStats} disabled={loading !== null}
          className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors">
          {loading === 'stats' ? 'Loading...' : 'Stats'}
        </button>
        <button onClick={loadFailed} disabled={loading !== null}
          className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium hover:bg-dark-accent2 disabled:opacity-50 transition-colors">
          {loading === 'failed' ? 'Loading...' : 'Failed Files'}
        </button>
      </div>

      {/* File table */}
      {files && files.length > 0 && display?.type === 'files' && (
        <div className="bg-dark-surface border border-dark-border rounded-lg overflow-hidden mb-4">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-dark-border text-left">
                <th className="px-4 py-2 text-xs text-dark-muted font-medium">Filename</th>
                <th className="px-4 py-2 text-xs text-dark-muted font-medium">Status</th>
                <th className="px-4 py-2 text-xs text-dark-muted font-medium">Chunks</th>
              </tr>
            </thead>
            <tbody>
              {files.slice(0, 50).map((f, i) => (
                <tr key={i} className="border-b border-dark-border last:border-0">
                  <td className="px-4 py-2 text-dark-text">{f.filename || f.name || '?'}</td>
                  <td className="px-4 py-2 text-dark-muted">{f.status || '?'}</td>
                  <td className="px-4 py-2 text-dark-muted">{f.chunks ?? '?'}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {files.length > 50 && (
            <p className="px-4 py-2 text-xs text-dark-muted">Showing 50 of {files.length}</p>
          )}
        </div>
      )}

      {/* Generic display for stats/failed */}
      {display && display.type !== 'files' && (
        <div className="bg-dark-surface border border-dark-border rounded-lg p-4">
          <h3 className="text-sm font-semibold mb-2 capitalize">{display.type}</h3>
          <JsonViewer data={display.data} />
        </div>
      )}

      {files !== null && files.length === 0 && display?.type === 'files' && (
        <p className="text-dark-muted text-sm">No files found</p>
      )}
    </div>
  );
}
