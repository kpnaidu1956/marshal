import { useState, useCallback, useMemo } from 'react';
import { useParams } from 'react-router-dom';
import { useApp } from '../context/AppContext';
import { api } from '../api/client';
import type { ToolParameterSchema } from '../api/types';

const catColors: Record<string, { bg: string; text: string }> = {
  read: { bg: 'bg-blue-400/15', text: 'text-blue-400' },
  write: { bg: 'bg-orange-400/15', text: 'text-orange-400' },
  search: { bg: 'bg-green-400/15', text: 'text-green-400' },
  report: { bg: 'bg-yellow-400/15', text: 'text-yellow-400' },
  sql: { bg: 'bg-red-400/15', text: 'text-red-400' },
};

function FieldInput({ toolName, name, schema, isRequired }: {
  toolName: string;
  name: string;
  schema: ToolParameterSchema;
  isRequired: boolean;
}) {
  const id = `field-${toolName}-${name}`;

  if (schema.enum) {
    return (
      <div>
        <label htmlFor={id} className="block text-xs font-medium text-dark-muted mb-1">
          {name} {isRequired && <span className="text-red-400">*</span>}
        </label>
        <select id={id} name={name} required={isRequired}
          className="w-full bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
        >
          <option value="">-- select --</option>
          {schema.enum.map((v) => <option key={v} value={v}>{v}</option>)}
        </select>
        {schema.description && <p className="text-[11px] text-dark-muted mt-1">{schema.description}</p>}
      </div>
    );
  }

  if (schema.type === 'boolean') {
    return (
      <div>
        <label htmlFor={id} className="block text-xs font-medium text-dark-muted mb-1">
          {name} {isRequired && <span className="text-red-400">*</span>}
        </label>
        <select id={id} name={name}
          className="w-full bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
        >
          <option value="">default</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </div>
    );
  }

  if (name === 'sql' || name === 'description' || name === 'content') {
    return (
      <div>
        <label htmlFor={id} className="block text-xs font-medium text-dark-muted mb-1">
          {name} {isRequired && <span className="text-red-400">*</span>}
        </label>
        <textarea id={id} name={name} required={isRequired}
          placeholder={schema.description || ''}
          className="w-full min-h-[80px] bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm font-mono resize-vertical focus:outline-none focus:border-dark-accent"
        />
      </div>
    );
  }

  const inputType = schema.type === 'integer' || schema.type === 'number' ? 'number' : 'text';
  const placeholder = schema.default !== undefined ? `default: ${schema.default}` : schema.description || '';

  return (
    <div>
      <label htmlFor={id} className="block text-xs font-medium text-dark-muted mb-1">
        {name} {isRequired && <span className="text-red-400">*</span>}
      </label>
      <input id={id} name={name} type={inputType} required={isRequired}
        placeholder={placeholder}
        min={schema.minimum}
        max={schema.maximum}
        className="w-full bg-dark-surface2 border border-dark-border text-dark-text px-3 py-2 rounded-md text-sm focus:outline-none focus:border-dark-accent"
      />
      {schema.description && <p className="text-[11px] text-dark-muted mt-1">{schema.description}</p>}
    </div>
  );
}

export function ToolPage() {
  const { name } = useParams<{ name: string }>();
  const { toolManifest, orgId, showResult } = useApp();
  const [loading, setLoading] = useState(false);

  const tool = useMemo(() => toolManifest.find((t) => t.name === name), [toolManifest, name]);

  const handleSubmit = useCallback(async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!tool) return;

    setLoading(true);
    const formData = new FormData(e.currentTarget);
    const params: Record<string, unknown> = { organization_id: orgId };

    for (const [key, value] of formData.entries()) {
      if (value === '') continue;
      const schema = tool.parameters?.properties?.[key];
      if (schema?.type === 'integer') params[key] = parseInt(value as string);
      else if (schema?.type === 'number') params[key] = parseFloat(value as string);
      else if (schema?.type === 'boolean') params[key] = value === 'true';
      else params[key] = value;
    }

    try {
      const start = performance.now();
      const result = await api.executeTool(tool.name, params);
      const elapsed = Math.round(performance.now() - start);
      showResult(
        result,
        result.summary || 'Tool executed',
        result.success,
        `${result.row_count ?? '-'} rows | ${result.execution_ms ?? '-'}ms server | ${elapsed}ms total`
      );
    } catch (err) {
      showResult({ error: (err as Error).message }, (err as Error).message, false, '');
    } finally {
      setLoading(false);
    }
  }, [tool, orgId, showResult]);

  if (!tool) {
    return (
      <div className="text-dark-muted">
        <p>Tool not found: <code className="text-dark-text">{name}</code></p>
        <p className="text-sm mt-2">Make sure the API is connected and the tool manifest is loaded.</p>
      </div>
    );
  }

  const props = tool.parameters?.properties || {};
  const required = tool.parameters?.required || [];
  const fields = Object.entries(props).filter(([n]) => n !== 'organization_id');
  const color = catColors[tool.category] || { bg: 'bg-dark-surface2', text: 'text-dark-muted' };

  return (
    <div>
      <div className="flex items-center gap-3 mb-1">
        <h2 className="text-xl font-semibold">{tool.name}</h2>
        <span className={`text-[11px] px-2 py-0.5 rounded font-semibold ${color.bg} ${color.text}`}>
          {tool.category}
        </span>
      </div>
      <p className="text-sm text-dark-muted mb-4">{tool.description}</p>

      <div className="bg-dark-surface border border-dark-border rounded-lg p-4">
        <form onSubmit={handleSubmit}>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {fields.map(([fieldName, schema]) => (
              <FieldInput
                key={fieldName}
                toolName={tool.name}
                name={fieldName}
                schema={schema}
                isRequired={required.includes(fieldName)}
              />
            ))}
          </div>
          <div className="flex gap-2 mt-4">
            <button
              type="submit"
              disabled={loading}
              className="px-4 py-2 bg-dark-accent text-white rounded-md text-sm font-medium hover:bg-dark-accent2 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {loading ? <><span className="spinner mr-2" />Running...</> : 'Execute'}
            </button>
            <button
              type="reset"
              className="px-4 py-2 bg-dark-surface2 text-dark-muted rounded-md text-sm hover:text-dark-text transition-colors"
            >
              Clear
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
