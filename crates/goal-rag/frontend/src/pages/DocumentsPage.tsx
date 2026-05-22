import { useState, useCallback, useEffect } from 'react';
import { DocumentList } from '../components/DocumentList';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';
import type { Document } from '../api/types';

export function DocumentsPage() {
  const { orgId } = useApp();
  const [documents, setDocuments] = useState<Document[]>([]);

  const loadDocuments = useCallback(async () => {
    try {
      const result = await api.listDocuments(orgId);
      setDocuments(result.documents);
    } catch (error) {
      console.error('Failed to load documents:', error);
    }
  }, [orgId]);

  useEffect(() => {
    loadDocuments();
  }, [loadDocuments]);

  const handleDelete = useCallback(async (id: string) => {
    try {
      await api.deleteDocument(id);
      loadDocuments();
    } catch (error) {
      console.error('Failed to delete document:', error);
    }
  }, [loadDocuments]);

  return (
    <div>
      <h2 className="text-xl font-semibold mb-1">Documents</h2>
      <p className="text-sm text-dark-muted mb-4">Manage uploaded documents</p>
      <DocumentList
        documents={documents}
        onDelete={handleDelete}
        onRefresh={loadDocuments}
      />
    </div>
  );
}
