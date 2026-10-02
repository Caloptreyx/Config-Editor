import { faSearch } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useState } from 'react';
import TextInput from '@/elements/input/TextInput.tsx';
import { nodeMatches } from '../lib/filter.ts';
import { type Format, isContainer, type Node } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import ContainerBody from './ContainerBody.tsx';
import { DocumentEditingContext } from './documentEditing.ts';
import FieldRow from './FieldRow.tsx';
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
      <div className='flex flex-col gap-4'>
        {isContainer(document) && (
          <TextInput
            placeholder={tExt('editor.filter', {})}
            leftSection={<FontAwesomeIcon icon={faSearch} />}
            value={filter}
            onChange={(e) => setFilter(e.currentTarget.value)}
          />
        )}

        {!isContainer(document) ? (
          <FieldRow label={tExt('editor.scalarRoot', {})}>
            <ScalarControl node={document} name={tExt('editor.scalarRoot', {})} path={[]} />
          </FieldRow>
        ) : nodeMatches(document, query) ? (
          <ContainerBody node={document} path={[]} query={query} />
        ) : (
          <p className='text-sm text-(--mantine-color-dimmed)'>{tExt('editor.noMatches', {})}</p>
        )}
      </div>
    </DocumentEditingContext>
  );
}
