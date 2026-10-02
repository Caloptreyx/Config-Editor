import { isContainer, type Node, type NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import CollapsibleGroup from './CollapsibleGroup.tsx';
import ContainerBody from './ContainerBody.tsx';
import FieldLabel from './FieldLabel.tsx';
import FieldRow from './FieldRow.tsx';
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
  const label = <FieldLabel name={name} kind={node.kind} />;
  const actions = <RowActions path={path} count={count} />;

  if (!isContainer(node)) {
    return (
      <FieldRow label={label} description={description} actions={actions}>
        <ScalarControl node={node} name={name} path={path} />
      </FieldRow>
    );
  }

  return (
    <CollapsibleGroup
      label={label}
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
