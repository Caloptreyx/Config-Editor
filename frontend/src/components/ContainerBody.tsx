import { entryMatches, keyMatches, nodeMatches } from '../lib/filter.ts';
import { allowedChildKinds } from '../lib/formats.ts';
import type { ContainerNode, NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import AddItemMenu from './AddItemMenu.tsx';
import AddKeyPopover from './AddKeyPopover.tsx';
import { useDocumentEditing } from './documentEditing.ts';
import ValueRow from './ValueRow.tsx';

// React keys for entries: the key plus its occurrence, so rows stay attached to their entry when others are removed
function entryRowKeys(keys: string[]): string[] {
  const seen = new Map<string, number>();
  return keys.map((key) => {
    const occurrence = seen.get(key) ?? 0;
    seen.set(key, occurrence + 1);
    return `${occurrence}:${key}`;
  });
}

// the children of an object or array, filtered by the normalized `query`, plus the add controls
export default function ContainerBody({ node, path, query }: { node: ContainerNode; path: NodePath; query: string }) {
  const { t: tExt } = useExtTranslations();
  const { format, readOnly } = useDocumentEditing();
  const kinds = allowedChildKinds(format, path);
  const canAdd = !readOnly && kinds.length > 0 && !query;

  if (node.kind === 'object') {
    const rowKeys = entryRowKeys(node.entries.map((entry) => entry.key));

    return (
      <div className='flex flex-col gap-3'>
        {node.entries.length === 0 && (
          <p className='text-xs text-(--mantine-color-dimmed)'>{tExt('editor.emptyObject', {})}</p>
        )}
        {node.entries.map(
          (entry, index) =>
            entryMatches(entry, query) && (
              <ValueRow
                key={rowKeys[index]}
                name={entry.key}
                node={entry.value}
                path={[...path, index]}
                description={entry.comment}
                query={keyMatches(entry, query) ? '' : query}
              />
            ),
        )}
        {canAdd && <AddKeyPopover path={path} kinds={kinds} existingKeys={node.entries.map((entry) => entry.key)} />}
      </div>
    );
  }

  return (
    <div className='flex flex-col gap-3'>
      {node.items.length === 0 && (
        <p className='text-xs text-(--mantine-color-dimmed)'>{tExt('editor.emptyArray', {})}</p>
      )}
      {node.items.map(
        (item, index) =>
          nodeMatches(item, query) && (
            <ValueRow
              key={index}
              name={tExt('editor.item', { index: index + 1 })}
              node={item}
              path={[...path, index]}
              description={null}
              count={node.items.length}
              query={query}
            />
          ),
      )}
      {canAdd && <AddItemMenu path={path} kinds={kinds} />}
    </div>
  );
}
