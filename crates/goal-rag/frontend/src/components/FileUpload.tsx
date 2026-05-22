import { useState, useCallback, memo } from 'react';
import { Upload, File, X, CheckCircle, AlertCircle, Loader2 } from 'lucide-react';
import { api } from '../api/client';

interface FileUploadProps {
  onUploadComplete: () => void;
  orgId?: string;
}

interface FileStatus {
  file: File;
  status: 'pending' | 'uploading' | 'success' | 'error';
  error?: string;
  chunks?: number;
}

const SUPPORTED_TYPES = [
  '.pdf', '.docx', '.doc', '.pptx', '.txt', '.md', '.xlsx', '.xls',
  '.html', '.htm', '.csv', '.rs', '.py', '.js', '.ts', '.tsx',
  '.jsx', '.go', '.java', '.cpp', '.c', '.h', '.json', '.yaml', '.yml'
];

export const FileUpload = memo(function FileUpload({ onUploadComplete, orgId }: FileUploadProps) {
  const [files, setFiles] = useState<FileStatus[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [isUploading, setIsUploading] = useState(false);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    addFiles(Array.from(e.dataTransfer.files));
  }, []);

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) addFiles(Array.from(e.target.files));
  }, []);

  const addFiles = (newFiles: File[]) => {
    setFiles(prev => [...prev, ...newFiles.map(file => ({ file, status: 'pending' as const }))]);
  };

  const removeFile = (index: number) => {
    setFiles(prev => prev.filter((_, i) => i !== index));
  };

  const uploadFiles = async () => {
    const pendingFiles = files.filter(f => f.status === 'pending');
    if (pendingFiles.length === 0) return;

    setIsUploading(true);
    setFiles(prev => prev.map(f => f.status === 'pending' ? { ...f, status: 'uploading' as const } : f));

    try {
      const result = await api.ingest(pendingFiles.map(f => f.file), orgId);
      setFiles(prev =>
        prev.map(f => {
          if (f.status !== 'uploading') return f;
          const error = result.errors.find(e => e.filename === f.file.name);
          if (error) return { ...f, status: 'error' as const, error: error.error };
          const doc = result.documents.find(d => d.filename === f.file.name);
          return { ...f, status: 'success' as const, chunks: doc?.total_chunks };
        })
      );
      onUploadComplete();
    } catch (error) {
      setFiles(prev =>
        prev.map(f =>
          f.status === 'uploading'
            ? { ...f, status: 'error' as const, error: error instanceof Error ? error.message : 'Upload failed' }
            : f
        )
      );
    } finally {
      setIsUploading(false);
    }
  };

  const clearCompleted = () => {
    setFiles(prev => prev.filter(f => f.status !== 'success'));
  };

  const formatFileSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="space-y-6">
      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        className={`border-2 border-dashed rounded-lg p-8 text-center transition-colors ${
          isDragging
            ? 'border-dark-accent bg-dark-accent/5'
            : 'border-dark-border hover:border-dark-muted'
        }`}
      >
        <Upload className={`w-12 h-12 mx-auto mb-4 ${isDragging ? 'text-dark-accent' : 'text-dark-muted'}`} />
        <p className="text-lg font-medium text-dark-text mb-1">
          {isDragging ? 'Drop files here' : 'Drag and drop files'}
        </p>
        <p className="text-sm text-dark-muted mb-4">or click to browse</p>
        <input type="file" multiple accept={SUPPORTED_TYPES.join(',')} onChange={handleFileSelect} className="hidden" id="file-input" />
        <label htmlFor="file-input" className="inline-flex items-center px-4 py-2 bg-dark-accent text-white rounded-lg hover:bg-dark-accent2 cursor-pointer transition-colors">
          Select Files
        </label>
        <p className="text-xs text-dark-muted mt-4">
          Supported: PDF, DOCX, PPTX, TXT, MD, XLSX, HTML, CSV, and code files
          <br />
          <span className="text-yellow-400">Note: Old .ppt files must be converted to .pptx</span>
        </p>
      </div>

      {files.length > 0 && (
        <div className="bg-dark-surface rounded-lg border border-dark-border divide-y divide-dark-border">
          <div className="px-4 py-3 flex items-center justify-between">
            <h3 className="font-medium text-dark-text">
              {files.length} file{files.length !== 1 ? 's' : ''} selected
            </h3>
            {files.some(f => f.status === 'success') && (
              <button onClick={clearCompleted} className="text-sm text-dark-muted hover:text-dark-text">
                Clear completed
              </button>
            )}
          </div>

          <div className="divide-y divide-dark-border max-h-80 overflow-y-auto">
            {files.map((fileStatus, index) => (
              <div key={`${fileStatus.file.name}-${index}`} className="px-4 py-3 flex items-center gap-4">
                <File className="w-5 h-5 text-dark-muted" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-dark-text truncate">{fileStatus.file.name}</p>
                  <p className="text-xs text-dark-muted">
                    {formatFileSize(fileStatus.file.size)}
                    {fileStatus.chunks && ` • ${fileStatus.chunks} chunks`}
                  </p>
                  {fileStatus.error && <p className="text-xs text-red-400 mt-1">{fileStatus.error}</p>}
                </div>
                <div className="flex items-center gap-2">
                  {fileStatus.status === 'pending' && (
                    <button onClick={() => removeFile(index)} className="p-1 text-dark-muted hover:text-dark-text"><X className="w-4 h-4" /></button>
                  )}
                  {fileStatus.status === 'uploading' && <Loader2 className="w-5 h-5 text-dark-accent animate-spin" />}
                  {fileStatus.status === 'success' && <CheckCircle className="w-5 h-5 text-green-400" />}
                  {fileStatus.status === 'error' && <AlertCircle className="w-5 h-5 text-red-400" />}
                </div>
              </div>
            ))}
          </div>

          {files.some(f => f.status === 'pending') && (
            <div className="px-4 py-3">
              <button
                onClick={uploadFiles}
                disabled={isUploading}
                className="w-full flex items-center justify-center gap-2 px-4 py-2 bg-dark-accent text-white rounded-lg hover:bg-dark-accent2 disabled:bg-dark-surface2 disabled:text-dark-muted disabled:cursor-not-allowed transition-colors"
              >
                {isUploading ? (
                  <><Loader2 className="w-4 h-4 animate-spin" />Uploading...</>
                ) : (
                  <><Upload className="w-4 h-4" />Upload {files.filter(f => f.status === 'pending').length} file{files.filter(f => f.status === 'pending').length !== 1 ? 's' : ''}</>
                )}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
});
