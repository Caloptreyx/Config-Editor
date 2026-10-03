import { faMagnifyingGlass, faXmark } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useState } from 'react';
import TextInput from '@/elements/input/TextInput.tsx';
import { nodeMatches } from '../lib/filter.ts';
import { type Format, isContainer, type Node } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import ContainerBody from './ContainerBody.tsx';
import { DocumentEditingContext } from './documentEditing.ts';
import { FieldRow } from './FieldRow.tsx';
import ScalarControl from './ScalarControl.tsx';

export default function VisualEditor({
  document,
  format,
  readOnly,
  onEdit,
}: {
  document: Node;
  format: Format;
  readOnly: boolean;
  onEdit: (update: (document: Node) => Node) => void;
}) {
  const { t: tExt } = useExtTranslations();
  const [filter, setFilter] = useState('');
  const query = filter.trim().toLowerCase();

  return (
    <DocumentEditingContext value={{ format, readOnly, edit: onEdit }}>
      {isContainer(document) && (
        <div className='shrink-0 border-b border-(--mantine-color-default-border) px-4 py-2'>
          <TextInput
            variant='unstyled'
            rightSectionPointerEvents='all'
            placeholder={tExt('editor.filter', {})}
            leftSection={<FontAwesomeIcon icon={faMagnifyingGlass} />}
            rightSection={
              filter && (
                <button
                  type='button'
                  className='mantine-focus-auto cursor-pointer'
                  aria-label={tExt('editor.clearFilter', {})}
                  onClick={() => setFilter('')}
                >
                  <FontAwesomeIcon icon={faXmark} />
                </button>
              )
            }
            value={filter}
            onChange={(e) => setFilter(e.currentTarget.value)}
          />
        </div>
      )}

      <div className='min-h-0 flex-1 overflow-y-auto p-4'>
        {!isContainer(document) ? (
          <div className='overflow-hidden rounded-md border border-(--mantine-color-default-border)'>
            <FieldRow label={tExt('editor.scalarRoot', {})}>
              <ScalarControl node={document} name={tExt('editor.scalarRoot', {})} path={[]} />
            </FieldRow>
          </div>
        ) : nodeMatches(document, query) ? (
          <ContainerBody node={document} path={[]} query={query} />
        ) : (
          <p className='py-6 text-center text-sm text-(--mantine-color-dimmed)'>{tExt('editor.noMatches', {})}</p>
        )}
      </div>
    </DocumentEditingContext>
  );
}
