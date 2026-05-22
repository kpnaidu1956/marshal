import { useState, useRef, useEffect, memo } from 'react';
import { Send, User, Bot, AlertTriangle, Clock } from 'lucide-react';
import { CitationCard } from './CitationCard';
import type { QueryResponse } from '../api/types';

interface Message {
  type: 'user' | 'assistant';
  content: string;
  response?: QueryResponse;
}

interface ChatInterfaceProps {
  chatHistory: Message[];
  onQuery: (question: string) => Promise<void>;
  hasDocuments: boolean;
}

export const ChatInterface = memo(function ChatInterface({ chatHistory, onQuery, hasDocuments }: ChatInterfaceProps) {
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [chatHistory]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;

    const question = input.trim();
    setInput('');
    setIsLoading(true);

    try {
      await onQuery(question);
    } finally {
      setIsLoading(false);
    }
  };

  const formatTime = (ms: number) => {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  };

  return (
    <div className="flex flex-col h-[calc(100vh-280px)] bg-dark-surface rounded-lg border border-dark-border">
      {/* Messages Area */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {chatHistory.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Bot className="w-16 h-16 text-dark-border mb-4" />
            <h3 className="text-lg font-medium text-dark-text mb-2">
              Ask questions about your documents
            </h3>
            <p className="text-sm text-dark-muted max-w-md">
              {hasDocuments
                ? 'Your documents are ready. Ask a question and I\'ll find the answer with source citations.'
                : 'Upload some documents first, then ask questions about their content.'}
            </p>
            {!hasDocuments && (
              <div className="mt-4 flex items-center gap-2 text-yellow-400 bg-yellow-400/10 px-4 py-2 rounded-lg">
                <AlertTriangle className="w-4 h-4" />
                <span className="text-sm">No documents uploaded yet</span>
              </div>
            )}
          </div>
        ) : (
          chatHistory.map((message, index) => (
            <div key={index} className={`flex gap-3 ${message.type === 'user' ? 'justify-end' : ''}`}>
              {message.type === 'assistant' && (
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-dark-accent/20 flex items-center justify-center">
                  <Bot className="w-5 h-5 text-dark-accent" />
                </div>
              )}
              <div
                className={`max-w-[80%] ${
                  message.type === 'user'
                    ? 'bg-dark-accent text-white rounded-2xl rounded-tr-sm px-4 py-2'
                    : 'space-y-3'
                }`}
              >
                {message.type === 'user' ? (
                  <p>{message.content}</p>
                ) : (
                  <>
                    <div className="bg-dark-surface2 rounded-2xl rounded-tl-sm px-4 py-3">
                      <p className="text-dark-text whitespace-pre-wrap">{message.content}</p>

                      {message.response && (
                        <div className="mt-3 pt-3 border-t border-dark-border flex items-center gap-4 text-xs text-dark-muted">
                          <span className="flex items-center gap-1">
                            <Clock className="w-3 h-3" />
                            {formatTime(message.response.processing_time_ms)}
                          </span>
                          <span>
                            {message.response.chunks_used} source{message.response.chunks_used !== 1 ? 's' : ''} used
                          </span>
                          <span>
                            Confidence: {Math.round(message.response.confidence * 100)}%
                          </span>
                        </div>
                      )}
                    </div>

                    {message.response?.citations && message.response.citations.length > 0 && (
                      <div className="space-y-2">
                        <p className="text-xs font-medium text-dark-muted px-1">Sources:</p>
                        {message.response.citations.map((citation, citIndex) => (
                          <CitationCard key={citIndex} citation={citation} index={citIndex + 1} />
                        ))}
                      </div>
                    )}
                  </>
                )}
              </div>
              {message.type === 'user' && (
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-dark-surface2 flex items-center justify-center">
                  <User className="w-5 h-5 text-dark-muted" />
                </div>
              )}
            </div>
          ))
        )}

        {isLoading && (
          <div className="flex gap-3">
            <div className="flex-shrink-0 w-8 h-8 rounded-full bg-dark-accent/20 flex items-center justify-center">
              <Bot className="w-5 h-5 text-dark-accent" />
            </div>
            <div className="bg-dark-surface2 rounded-2xl rounded-tl-sm px-4 py-3">
              <div className="flex items-center gap-2">
                <div className="w-2 h-2 bg-dark-muted rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                <div className="w-2 h-2 bg-dark-muted rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                <div className="w-2 h-2 bg-dark-muted rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input Area */}
      <div className="border-t border-dark-border p-4">
        <form onSubmit={handleSubmit} className="flex gap-3">
          <input
            type="text"
            value={input}
            onChange={e => setInput(e.target.value)}
            placeholder={hasDocuments ? 'Ask a question about your documents...' : 'Upload documents first...'}
            disabled={!hasDocuments || isLoading}
            className="flex-1 px-4 py-2 bg-dark-surface2 border border-dark-border rounded-lg text-dark-text placeholder-dark-muted focus:outline-none focus:border-dark-accent disabled:opacity-50"
          />
          <button
            type="submit"
            disabled={!input.trim() || !hasDocuments || isLoading}
            className="px-4 py-2 bg-dark-accent text-white rounded-lg hover:bg-dark-accent2 disabled:bg-dark-surface2 disabled:text-dark-muted disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            <Send className="w-4 h-4" />
            Send
          </button>
        </form>
      </div>
    </div>
  );
});
