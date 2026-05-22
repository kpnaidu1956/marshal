import { memo } from 'react';
import { useApp } from '../context/AppContext';

export const TopBar = memo(function TopBar() {
  const { apiUrl, setApiUrl, orgId, setOrgId, isConnected } = useApp();

  return (
    <div className="flex items-center gap-3 px-5 py-3 bg-dark-surface border-b border-dark-border">
      <label className="text-xs text-dark-muted">API:</label>
      <input
        type="text"
        value={apiUrl}
        onChange={(e) => setApiUrl(e.target.value)}
        className="w-64 bg-dark-surface2 border border-dark-border text-dark-text px-3 py-1.5 rounded-md text-sm focus:outline-none focus:border-dark-accent"
      />
      <label className="text-xs text-dark-muted">Org:</label>
      <select
        value={orgId}
        onChange={(e) => setOrgId(e.target.value)}
        className="min-w-[220px] bg-dark-surface2 border border-dark-border text-dark-text px-3 py-1.5 rounded-md text-sm focus:outline-none focus:border-dark-accent"
      >
        <option value="demo-org">demo-org</option>
        <option value="demo-org">demo-org</option>
      </select>
      <div className="flex items-center gap-2 ml-2">
        <span className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-red-400'}`} />
        <span className="text-xs text-dark-muted">{isConnected ? 'Connected' : 'Offline'}</span>
      </div>
    </div>
  );
});
