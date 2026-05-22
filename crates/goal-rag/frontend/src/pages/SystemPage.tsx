import { useState, useCallback } from 'react';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';
import { JsonViewer } from '../components/JsonViewer';

export function SystemPage() {
  const { showResult } = useApp();
  const [display, setDisplay] = useState<{ type: string; data: unknown } | null>(null);
  const [loading, setLoading] = useState<string | null>(null);

  const load = useCallback(async (type: string) => {
    setLoading(type);
    try {
      let data: unknown;
      switch (type) {
        case 'health': data = await api.healthCheck(); break;
        case 'info': data = await api.getInfo(); break;
        case 'capabilities': data = await api.getCapabilities(); break;
        case 'parsers': data = await api.getParsersStatus(); break;
      }
      setDisplay({ type, data });
      showResult(data, `${type} info`, true, '');
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(null);
    }
  }, [showResult]);

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">System</h2>
      <p className="text-sm text-dark-muted mb-4">Health checks, capabilities, and server info</p>

      <div className="flex gap-2 mb-4">
        {['health', 'info', 'capabilities', 'parsers'].map((type) => (
          <button
            key={type}
            onClick={() => load(type)}
            disabled={loading !== null}
            className="px-3 py-1.5 bg-dark-accent text-white rounded-md text-xs font-medium capitalize hover:bg-dark-accent2 disabled:opacity-50 transition-colors"
          >
            {loading === type ? 'Loading...' : type}
          </button>
        ))}
      </div>

      {display && (
        <div className="bg-dark-surface border border-dark-border rounded-lg p-4">
          <h3 className="text-sm font-semibold mb-2 capitalize">{display.type}</h3>
          <JsonViewer data={display.data} />
        </div>
      )}
    </div>
  );
}
