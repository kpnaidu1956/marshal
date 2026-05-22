import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { TopBar } from './TopBar';
import { ResultPanel } from './ResultPanel';

export function Layout() {
  return (
    <div className="flex h-screen overflow-hidden bg-dark-bg text-dark-text">
      <Sidebar />
      <div className="flex-1 flex flex-col overflow-hidden">
        <TopBar />
        <div className="flex-1 overflow-y-auto p-5">
          <Outlet />
        </div>
        <ResultPanel />
      </div>
    </div>
  );
}
