import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from 'react';
import { api } from '../api/client';
import type { ToolDefinition } from '../api/types';

interface ResultData {
  data: unknown;
  summary: string;
  success: boolean;
  meta: string;
}

interface AppContextType {
  apiUrl: string;
  setApiUrl: (url: string) => void;
  orgId: string;
  setOrgId: (id: string) => void;
  isConnected: boolean;
  toolManifest: ToolDefinition[];
  result: ResultData | null;
  showResult: (data: unknown, summary: string, success: boolean, meta: string) => void;
}

const AppContext = createContext<AppContextType | null>(null);

function detectApiUrl(): string {
  const loc = window.location;
  if (loc.port === '8080') return loc.origin;
  if (loc.hostname !== 'localhost' && loc.hostname !== '127.0.0.1') {
    return `${loc.protocol}//${loc.hostname}:8080`;
  }
  return 'http://localhost:8080';
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [apiUrl, setApiUrlState] = useState(detectApiUrl);
  const [orgId, setOrgId] = useState('demo-org');
  const [isConnected, setIsConnected] = useState(false);
  const [toolManifest, setToolManifest] = useState<ToolDefinition[]>([]);
  const [result, setResult] = useState<ResultData | null>(null);

  const setApiUrl = useCallback((url: string) => {
    const clean = url.replace(/\/+$/, '');
    setApiUrlState(clean);
    api.setBaseUrl(clean);
  }, []);

  const showResult = useCallback((data: unknown, summary: string, success: boolean, meta: string) => {
    setResult({ data, summary, success, meta });
  }, []);

  // Initialize API client and fetch manifest
  useEffect(() => {
    api.setBaseUrl(apiUrl);

    const loadManifest = async () => {
      try {
        const manifest = await api.getToolManifest();
        setToolManifest(manifest);
        setIsConnected(true);
      } catch {
        setIsConnected(false);
      }
    };

    loadManifest();
  }, [apiUrl]);

  // Health check polling
  useEffect(() => {
    const check = async () => {
      const ok = await api.healthCheck();
      setIsConnected(ok);
    };
    const interval = setInterval(check, 30000);
    return () => clearInterval(interval);
  }, [apiUrl]);

  return (
    <AppContext.Provider value={{ apiUrl, setApiUrl, orgId, setOrgId, isConnected, toolManifest, result, showResult }}>
      {children}
    </AppContext.Provider>
  );
}

export function useApp(): AppContextType {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error('useApp must be used within AppProvider');
  return ctx;
}
