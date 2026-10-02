import { faFileLines, faFolder } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import classNames from 'classnames';
import type { ReactNode } from 'react';
import UnstyledButton from '@/elements/buttons/UnstyledButton.tsx';
import type { Format } from '../lib/node.ts';
import FormatBadge from './FormatBadge.tsx';

// one clickable folder or config file line of the side panel; `actions` render outside the button
export default function FileListRow({
  name,
  title,
  directory = false,
  format,
  active = false,
  actions,
  onClick,
}: {
  name: string;
  title?: string;
  directory?: boolean;
  format: Format | null;
  active?: boolean;
  actions?: ReactNode;
  onClick: () => void;
}) {
  return (
    <div
      className={classNames(
        'flex items-center gap-1 rounded-sm pr-1',
        active ? 'bg-(--mantine-color-default-hover)' : 'hover:bg-(--mantine-color-default-hover)',
      )}
    >
      <UnstyledButton
        title={title ?? name}
        onClick={onClick}
        className='flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-sm'
      >
        <FontAwesomeIcon
          icon={directory ? faFolder : faFileLines}
          className='w-4 shrink-0'
          style={{ color: directory ? 'var(--mantine-primary-color-filled)' : 'var(--mantine-color-dimmed)' }}
        />
        <span className='min-w-0 flex-1 truncate'>{name}</span>
        {format && <FormatBadge format={format} />}
      </UnstyledButton>
      {actions}
    </div>
  );
}
