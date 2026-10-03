import classNames from 'classnames';
import type { ReactNode } from 'react';
import { entryMatches, keyMatches, nodeMatches } from '../lib/filter.ts';
import { allowedChildKinds } from '../lib/formats.ts';
import { type ContainerNode, isContainer, type NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import AddItemMenu from './AddItemMenu.tsx';
import AddKeyPopover from './AddKeyPopover.tsx';
import { useDocumentEditing } from './documentEditing.ts';
import ValueRow from './ValueRow.tsx';

const BOX = 'overflow-hidden rounded-md border border-(--mantine-color-default-border)';

// React keys for entries: the key plus its occurrence, so rows stay attached to their entry when others are removed
function entryRowKeys(keys: string[]): string[] {
  const seen = new Map<string, number>();
  return keys.map((key) => {
    const occurrence = seen.get(key) ?? 0;
    seen.set(key, occurrence + 1);
    return `${occurrence}:${key}`;
  });
}

interface Child {
  key: string;
  group: boolean;
  element: ReactNode;
}

// Lays out the rendered children. At the root, runs of plain values share a bordered box and every group is its
// own card. Inside a group card, plain rows are divided by lines and nested groups are inset cards.
function Layout({
  root,
  list,
  children,
  footer,
}: {
  root: boolean;
  list: boolean;
  children: Child[];
  footer: ReactNode;
}) {
  if (root) {
    const blocks: Child[][] = [];
    for (const child of children) {
      const last = blocks[blocks.length - 1];
      if (!child.group && last && !last[0].group) last.push(child);
      else blocks.push([child]);
    }

    return (
      <div className='flex flex-col gap-3'>
        {blocks.map((block) =>
          block[0].group ? (
            <div key={block[0].key}>{block[0].element}</div>
          ) : (
            <div
              key={block[0].key}
              className={classNames(BOX, list ? 'py-1.5' : 'divide-y divide-(--mantine-color-default-border)')}
            >
              {block.map((child) => (
                <div key={child.key}>{child.element}</div>
              ))}
            </div>
          ),
        )}
        {footer}
      </div>
    );
  }

  return (
    <div className={classNames(list ? 'py-1.5' : 'divide-y divide-(--mantine-color-default-border)')}>
      {children.map((child) => (
        <div key={child.key} className={child.group ? (list ? 'px-4 py-1.5' : 'p-3') : undefined}>
          {child.element}
        </div>
      ))}
      {footer && <div className={list ? 'px-4 py-1.5' : 'p-3'}>{footer}</div>}
    </div>
  );
}

// the children of an object or array, filtered by the normalized `query`, plus the add controls
export default function ContainerBody({ node, path, query }: { node: ContainerNode; path: NodePath; query: string }) {
  const { t: tExt } = useExtTranslations();
  const { format, readOnly } = useDocumentEditing();
  const kinds = allowedChildKinds(format, path);
  const canAdd = !readOnly && kinds.length > 0 && !query;
  const root = path.length === 0;

  const children: Child[] = [];
  if (node.kind === 'object') {
    const rowKeys = entryRowKeys(node.entries.map((entry) => entry.key));
    node.entries.forEach((entry, index) => {
      if (!entryMatches(entry, query)) return;
      children.push({
        key: rowKeys[index],
        group: isContainer(entry.value),
        element: (
          <ValueRow
            name={entry.key}
            node={entry.value}
            path={[...path, index]}
            description={entry.comment}
            query={keyMatches(entry, query) ? '' : query}
          />
        ),
      });
    });
  } else {
    node.items.forEach((item, index) => {
      if (!nodeMatches(item, query)) return;
      children.push({
        key: String(index),
        group: isContainer(item),
        element: (
          <ValueRow
            name={tExt('editor.item', { index: index + 1 })}
            node={item}
            path={[...path, index]}
            description={null}
            count={node.items.length}
            query={query}
          />
        ),
      });
    });
  }

  const empty = node.kind === 'object' ? node.entries.length === 0 : node.items.length === 0;
  const footer = canAdd ? (
    node.kind === 'object' ? (
      <AddKeyPopover path={path} kinds={kinds} existingKeys={node.entries.map((entry) => entry.key)} />
    ) : (
      <AddItemMenu path={path} kinds={kinds} />
    )
  ) : null;

  return (
    <>
      {empty && (
        <p className={classNames('text-xs text-(--mantine-color-dimmed)', root ? 'pb-3' : 'px-4 pt-3')}>
          {node.kind === 'object' ? tExt('editor.emptyObject', {}) : tExt('editor.emptyArray', {})}
        </p>
      )}
      <Layout root={root} list={node.kind === 'array'} footer={footer}>
        {children}
      </Layout>
    </>
  );
}
