import Tooltip from '@/elements/overlays/Tooltip.tsx';
import type { NodeKind } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';

const MARKS: Record<NodeKind, string> = {
  string: 'abc',
  integer: '123',
  float: '1.5',
  boolean: 'y/n',
  null: 'null',
  datetime: 'date',
  array: '[ ]',
  object: '{ }',
};

// a compact value type marker; the full type name is in the tooltip
export default function KindMark({ kind }: { kind: NodeKind }) {
  const { t: tExt } = useExtTranslations();

  return (
    <Tooltip label={tExt(`kinds.${kind}`, {})}>
      <span
        aria-label={tExt(`kinds.${kind}`, {})}
        className='shrink-0 rounded bg-(--mantine-color-default-hover) px-1.5 py-px font-mono text-[10px] leading-4 text-(--mantine-color-dimmed)'
      >
        {MARKS[kind]}
      </span>
    </Tooltip>
  );
}
