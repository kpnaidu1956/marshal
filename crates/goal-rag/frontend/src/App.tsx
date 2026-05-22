import { HashRouter, Routes, Route } from 'react-router-dom';
import { AppProvider } from './context/AppContext';
import { Layout } from './components/Layout';
import { WelcomePage } from './pages/WelcomePage';
import { ToolPage } from './pages/ToolPage';
import { RagQueryPage } from './pages/RagQueryPage';
import { AnalyticsPage } from './pages/AnalyticsPage';
import { FilesPage } from './pages/FilesPage';
import { SystemPage } from './pages/SystemPage';
import { ChatPage } from './pages/ChatPage';
import { DocumentsPage } from './pages/DocumentsPage';

function App() {
  return (
    <HashRouter>
      <AppProvider>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<WelcomePage />} />
            <Route path="/tools/:name" element={<ToolPage />} />
            <Route path="/rag" element={<RagQueryPage />} />
            <Route path="/chat" element={<ChatPage />} />
            <Route path="/analytics" element={<AnalyticsPage />} />
            <Route path="/files" element={<FilesPage />} />
            <Route path="/documents" element={<DocumentsPage />} />
            <Route path="/system" element={<SystemPage />} />
          </Route>
        </Routes>
      </AppProvider>
    </HashRouter>
  );
}

export default App;
