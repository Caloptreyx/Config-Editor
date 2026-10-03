import { isContainer, type Node, type NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import CollapsibleGroup from './CollapsibleGroup.tsx';
import ContainerBody from './ContainerBody.tsx';
import { FieldName, FieldRow, ItemRow } from './FieldRow.tsx';
import KindMark from './KindMark.tsx';
import RowActions from './RowActions.tsx';
import ScalarControl from './ScalarControl.tsx';

// one object entry or array item (`count` is the array length) with its value editor
export default function ValueRow({
  name,
  node,
  path,
  description,
  count,
  query,
}: {
  name: string;
  node: Node;
  path: NodePath;
  description: string | null;
  count?: number;
  query: string;
}) {
  const { tItem } = useExtTranslations();
  const actions = <RowActions path={path} count={count} />;
  const index = path[path.length - 1];

  if (!isContainer(node)) {
    const control = <ScalarControl node={node} name={name} path={path} />;

    return count === undefined ? (
      <FieldRow
        label={<FieldName name={name} mark={<KindMark kind={node.kind} />} />}
        description={description}
        actions={actions}
      >
        {control}
      </FieldRow>
    ) : (
      <ItemRow index={index + 1} actions={actions}>
        {control}
      </ItemRow>
    );
  }

  return (
    <CollapsibleGroup
      label={<FieldName name={name} mark={<KindMark kind={node.kind} />} />}
      summary={node.kind === 'array' ? tItem('item', node.items.length) : tItem('key', node.entries.length)}
      description={description}
      actions={actions}
      defaultOpen={path.length <= 1}
      forceOpen={query !== ''}
    >
      <ContainerBody node={node} path={path} query={query} />
    </CollapsibleGroup>
  );
}
