import MonacoEditor from '@/elements/editors/MonacoEditor.tsx';
import { registerTomlLanguage } from '@/lib/editor/monaco.ts';
import { monacoLanguage } from '../lib/formats.ts';
import type { Format } from '../lib/node.ts';

export default function RawEditor({
  path,
  format,
  content,
  readOnly,
  onChange,
}: {
  path: string;
  format: Format;
  content: string;
  readOnly: boolean;
  onChange: (content: string) => void;
}) {
  return (
    <div className='m-4 min-h-80 flex-1 overflow-hidden rounded-md border border-(--mantine-color-default-border)'>
      <MonacoEditor
        height='100%'
        width='100%'
        path={`config-editor:${path}`}
        language={monacoLanguage(format)}
        value={content}
        beforeMount={registerTomlLanguage}
        onChange={(value) => onChange(value ?? '')}
        options={{
          readOnly,
          minimap: { enabled: false },
          stickyScroll: { enabled: false },
          codeLens: false,
          scrollBeyondLastLine: false,
          fixedOverflowWidgets: true,
        }}
      />
    </div>
  );
}
