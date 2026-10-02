import type { NodeKind } from '../lib/node.ts';
import KindBadge from './KindBadge.tsx';

export default function FieldLabel({ name, kind }: { name: string; kind: NodeKind }) {
  return (
    <span className='inline-flex min-w-0 items-center gap-2'>
      <span className='truncate font-mono'>{name}</span>
      <KindBadge kind={kind} />
    </span>
  );
}
