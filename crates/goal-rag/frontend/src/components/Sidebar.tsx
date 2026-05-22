import { memo, useMemo } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import {
  Search, FileText, BarChart3, HardDrive, Activity,
  MessageSquare, Database, BookOpen, PenTool, FileCode,
  LayoutDashboard,
} from 'lucide-react';
import { useApp } from '../context/AppContext';

const categoryIcons: Record<string, typeof Search> = {
  read: BookOpen,
  write: PenTool,
  search: Search,
  report: BarChart3,
  sql: FileCode,
};

const categoryOrder = ['read', 'write', 'search', 'report', 'sql'];

export const Sidebar = memo(function Sidebar() {
  const { toolManifest } = useApp();
  const navigate = useNavigate();
  const location = useLocation();

  const toolsByCategory = useMemo(() => {
    const cats: Record<string, typeof toolManifest> = {};
    toolManifest.forEach((t) => {
      const cat = t.category || 'other';
      if (!cats[cat]) cats[cat] = [];
      cats[cat].push(t);
    });
    return cats;
  }, [toolManifest]);

  const isActive = (path: string) => location.pathname === path || location.hash === `#${path}`;

  return (
    <div className="w-60 bg-dark-surface border-r border-dark-border flex flex-col flex-shrink-0 overflow-y-auto">
      {/* Header */}
      <div className="px-4 py-4 border-b border-dark-border">
        <h1 className="text-base font-semibold text-dark-text">Goal-RAG</h1>
        <p className="text-[11px] text-dark-muted mt-0.5">Testing Dashboard</p>
      </div>

      {/* Dashboard */}
      <div className="py-1 border-b border-dark-border">
        <button
          onClick={() => navigate('/')}
          className={`flex items-center gap-2 w-full px-4 py-2 text-sm transition-colors ${
            isActive('/') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <LayoutDashboard className="w-4 h-4" />
          Dashboard
        </button>
      </div>

      {/* Tool categories */}
      {categoryOrder.map((cat) => {
        const tools = toolsByCategory[cat];
        if (!tools) return null;
        const Icon = categoryIcons[cat] || Database;
        return (
          <div key={cat} className="py-1 border-b border-dark-border">
            <div className="flex items-center gap-2 px-4 py-1">
              <Icon className="w-3 h-3 text-dark-muted" />
              <span className="text-[10px] font-bold uppercase tracking-wider text-dark-muted">
                {cat} Tools
              </span>
              <span className="text-[10px] bg-dark-surface2 text-dark-muted px-1.5 py-0 rounded-full ml-auto">
                {tools.length}
              </span>
            </div>
            {tools.map((tool) => (
              <button
                key={tool.name}
                onClick={() => navigate(`/tools/${tool.name}`)}
                className={`block w-full text-left px-4 pl-8 py-1.5 text-[13px] transition-colors ${
                  isActive(`/tools/${tool.name}`)
                    ? 'bg-dark-accent2 text-white'
                    : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
                }`}
              >
                {tool.name}
              </button>
            ))}
          </div>
        );
      })}

      {/* Fixed sections */}
      <div className="py-1 border-b border-dark-border">
        <div className="px-4 py-1">
          <span className="text-[10px] font-bold uppercase tracking-wider text-dark-muted">RAG</span>
        </div>
        <button
          onClick={() => navigate('/rag')}
          className={`flex items-center gap-2 w-full px-4 pl-8 py-1.5 text-[13px] transition-colors ${
            isActive('/rag') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <Search className="w-3.5 h-3.5" />
          Query Documents
        </button>
        <button
          onClick={() => navigate('/chat')}
          className={`flex items-center gap-2 w-full px-4 pl-8 py-1.5 text-[13px] transition-colors ${
            isActive('/chat') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <MessageSquare className="w-3.5 h-3.5" />
          Chat
        </button>
      </div>

      <div className="py-1 border-b border-dark-border">
        <button
          onClick={() => navigate('/analytics')}
          className={`flex items-center gap-2 w-full px-4 py-1.5 text-[13px] transition-colors ${
            isActive('/analytics') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <BarChart3 className="w-3.5 h-3.5" />
          Analytics
        </button>
      </div>

      <div className="py-1 border-b border-dark-border">
        <button
          onClick={() => navigate('/files')}
          className={`flex items-center gap-2 w-full px-4 py-1.5 text-[13px] transition-colors ${
            isActive('/files') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <HardDrive className="w-3.5 h-3.5" />
          Files
        </button>
        <button
          onClick={() => navigate('/documents')}
          className={`flex items-center gap-2 w-full px-4 pl-8 py-1.5 text-[13px] transition-colors ${
            isActive('/documents') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <FileText className="w-3.5 h-3.5" />
          Documents
        </button>
      </div>

      <div className="py-1">
        <button
          onClick={() => navigate('/system')}
          className={`flex items-center gap-2 w-full px-4 py-1.5 text-[13px] transition-colors ${
            isActive('/system') ? 'bg-dark-accent2 text-white' : 'text-dark-muted hover:bg-dark-surface2 hover:text-dark-text'
          }`}
        >
          <Activity className="w-3.5 h-3.5" />
          System
        </button>
      </div>
    </div>
  );
});
