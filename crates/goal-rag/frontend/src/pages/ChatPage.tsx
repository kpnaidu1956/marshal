import { useState, useCallback, useEffect } from 'react';
import { ChatInterface } from '../components/ChatInterface';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';
import type { Document, QueryResponse } from '../api/types';

export function ChatPage() {
  const { orgId } = useApp();
  const [documents, setDocuments] = useState<Document[]>([]);
  const [chatHistory, setChatHistory] = useState<Array<{
    type: 'user' | 'assistant';
    content: string;
    response?: QueryResponse;
  }>>([]);

  useEffect(() => {
    api.listDocuments(orgId).then((r) => setDocuments(r.documents)).catch(() => {});
  }, [orgId]);

  const handleQuery = useCallback(async (question: string) => {
    setChatHistory((prev) => [...prev, { type: 'user', content: question }]);
    try {
      const response = await api.query({ question, organization_id: orgId, top_k: 5, similarity_threshold: 0.3 });
      setChatHistory((prev) => [...prev, { type: 'assistant', content: response.answer, response }]);
    } catch (error) {
      setChatHistory((prev) => [...prev, {
        type: 'assistant',
        content: `Error: ${error instanceof Error ? error.message : 'Failed to get response'}`,
      }]);
    }
  }, [orgId]);

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">Chat</h2>
      <p className="text-sm text-dark-muted mb-4">Document Q&A with citations</p>
      <ChatInterface
        chatHistory={chatHistory}
        onQuery={handleQuery}
        hasDocuments={documents.length > 0}
      />
    </div>
  );
}
