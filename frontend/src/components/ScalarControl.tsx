import Switch from '@/elements/input/Switch.tsx';
import TextArea from '@/elements/input/TextArea.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import { inferFlatScalar, isFlatFormat } from '../lib/formats.ts';
import { isValidScalar, type Node, type NodePath, type TextNode, updateAt } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import { useDocumentEditing } from './documentEditing.ts';
import NullValue from './NullValue.tsx';

const INVALID_MESSAGE_KEYS = {
  integer: 'editor.invalidInteger',
  float: 'editor.invalidFloat',
  datetime: 'editor.invalidDatetime',
} as const;

function TextControl({ node, name, onChange }: { node: TextNode; name: string; onChange: (node: Node) => void }) {
  const { t: tExt } = useExtTranslations();
  const { format, readOnly } = useDocumentEditing();

  // flat formats store text, so the kind follows from what is typed and is never invalid
  const nodeFromText = (text: string): Node =>
    isFlatFormat(format) ? inferFlatScalar(text) : { kind: node.kind, value: text };
  const error = node.kind !== 'string' && !isValidScalar(node) ? tExt(INVALID_MESSAGE_KEYS[node.kind], {}) : undefined;

  return node.kind === 'string' && node.value.includes('\n') ? (
    <TextArea
      aria-label={name}
      autosize
      minRows={2}
      maxRows={16}
      readOnly={readOnly}
      value={node.value}
      onChange={(e) => onChange(nodeFromText(e.currentTarget.value))}
    />
  ) : (
    <TextInput
      aria-label={name}
      readOnly={readOnly}
      error={error}
      className={node.kind === 'string' ? undefined : 'font-mono'}
      inputMode={node.kind === 'integer' || node.kind === 'float' ? 'decimal' : undefined}
      value={node.value}
      onChange={(e) => onChange(nodeFromText(e.currentTarget.value))}
    />
  );
}

// the input for a non-container value; integers and floats stay text so no precision is lost
export default function ScalarControl({ node, name, path }: { node: Node; name: string; path: NodePath }) {
  const { readOnly, edit } = useDocumentEditing();
  const set = (next: Node) => edit((document) => updateAt(document, path, () => next));

  switch (node.kind) {
    case 'boolean':
      return (
        <Switch
          aria-label={name}
          checked={node.value}
          disabled={readOnly}
          onChange={(e) => set({ kind: 'boolean', value: e.currentTarget.checked })}
        />
      );
    case 'null':
      return <NullValue path={path} />;
    case 'array':
    case 'object':
      return null;
    default:
      return <TextControl node={node} name={name} onChange={set} />;
  }
}
