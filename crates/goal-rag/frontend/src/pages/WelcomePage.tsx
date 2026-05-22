import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useApp } from '../context/AppContext';

export function WelcomePage() {
  const { toolManifest, isConnected } = useApp();
  const navigate = useNavigate();

  const stats = useMemo(() => {
    const cats: Record<string, number> = {};
    toolManifest.forEach((t) => {
      const cat = t.category || 'other';
      cats[cat] = (cats[cat] || 0) + 1;
    });
    return cats;
  }, [toolManifest]);

  const catColors: Record<string, string> = {
    read: 'text-blue-400',
    write: 'text-orange-400',
    search: 'text-green-400',
    report: 'text-yellow-400',
    sql: 'text-red-400',
  };

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">Dashboard</h2>
      <p className="text-sm text-dark-muted mb-6">
        {isConnected ? 'Connected to Goal-RAG API' : 'Not connected — check API URL'}
      </p>

      {/* Stats grid */}
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3 mb-8">
        <div className="bg-dark-surface2 rounded-lg p-4 text-center">
          <div className="text-2xl font-bold text-dark-accent">{toolManifest.length}</div>
          <div className="text-xs text-dark-muted mt-1">Total Tools</div>
        </div>
        {Object.entries(stats).map(([cat, count]) => (
          <div key={cat} className="bg-dark-surface2 rounded-lg p-4 text-center">
            <div className={`text-2xl font-bold ${catColors[cat] || 'text-dark-text'}`}>{count}</div>
            <div className="text-xs text-dark-muted mt-1">{cat}</div>
          </div>
        ))}
      </div>

      {/* Quick links */}
      <h3 className="text-sm font-semibold text-dark-muted uppercase tracking-wider mb-3">Quick Actions</h3>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
        {[
          { label: 'List Tasks', path: '/tools/list_tasks', desc: 'View paginated tasks' },
          { label: 'Sprint Report', path: '/tools/sprint_report', desc: 'Team velocity & status' },
          { label: 'RAG Query', path: '/rag', desc: 'Query documents with AI' },
          { label: 'Embedding Stats', path: '/analytics', desc: 'Entity embedding analytics' },
        ].map((link) => (
          <button
            key={link.path}
            onClick={() => navigate(link.path)}
            className="bg-dark-surface border border-dark-border rounded-lg p-4 text-left hover:border-dark-accent transition-colors"
          >
            <div className="text-sm font-medium text-dark-text">{link.label}</div>
            <div className="text-xs text-dark-muted mt-1">{link.desc}</div>
          </button>
        ))}
      </div>
    </div>
  );
}
