import { memo } from 'react';

function syntaxHighlight(json: string): string {
  return json.replace(
    /("(\\u[\da-fA-F]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?|null)/g,
    (match) => {
      let cls = 'json-number';
      if (/^"/.test(match)) {
        cls = /:$/.test(match) ? 'json-key' : 'json-string';
      } else if (/true|false/.test(match)) {
        cls = 'json-bool';
      } else if (/null/.test(match)) {
        cls = 'json-null';
      }
      return `<span class="${cls}">${match}</span>`;
    }
  );
}

export const JsonViewer = memo(function JsonViewer({ data }: { data: unknown }) {
  const json = JSON.stringify(data, null, 2);
  return (
    <pre
      className="font-mono text-xs leading-relaxed whitespace-pre-wrap break-words"
      dangerouslySetInnerHTML={{ __html: syntaxHighlight(json) }}
    />
  );
});
