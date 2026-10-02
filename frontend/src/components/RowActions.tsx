import { faArrowDown, faArrowUp, faTrash, type IconDefinition } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Group from '@/elements/layout/Group.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import { moveAt, type NodePath, removeAt } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import { useDocumentEditing } from './documentEditing.ts';

function Action({
  icon,
  label,
  color = 'gray',
  disabled,
  onClick,
}: {
  icon: IconDefinition;
  label: string;
  color?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <Tooltip label={label}>
      <ActionIcon
        variant='subtle'
        color={color}
        disabled={disabled}
        aria-label={label}
        onClick={(e) => {
          e.stopPropagation();
          onClick();
        }}
      >
        <FontAwesomeIcon icon={icon} />
      </ActionIcon>
    </Tooltip>
  );
}

// remove button of an entry or array item; array items (`count` given) can also move up and down
export default function RowActions({ path, count }: { path: NodePath; count?: number }) {
  const { t: tExt } = useExtTranslations();
  const { readOnly, edit } = useDocumentEditing();
  if (readOnly) return null;

  const index = path[path.length - 1];

  return (
    <Group gap={2} wrap='nowrap' className='shrink-0'>
      {count !== undefined && (
        <>
          <Action
            icon={faArrowUp}
            label={tExt('editor.moveUp', {})}
            disabled={index === 0}
            onClick={() => edit((document) => moveAt(document, path, -1))}
          />
          <Action
            icon={faArrowDown}
            label={tExt('editor.moveDown', {})}
            disabled={index === count - 1}
            onClick={() => edit((document) => moveAt(document, path, 1))}
          />
        </>
      )}
      <Action
        icon={faTrash}
        label={tExt('editor.remove', {})}
        color='red'
        onClick={() => edit((document) => removeAt(document, path))}
      />
    </Group>
  );
}
