import Badge from '@/elements/data-display/Badge.tsx';
import type { NodeKind } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';

export default function KindBadge({ kind }: { kind: NodeKind }) {
  const { t: tExt } = useExtTranslations();

  return (
    <Badge size='xs' variant='light' color='gray' className='shrink-0'>
      {tExt(`kinds.${kind}`, {})}
    </Badge>
  );
}
