import { faFolder } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import classNames from 'classnames';
import type { ReactNode } from 'react';
import type { Format } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import { FormatIcon } from './FormatBadge.tsx';

// one clickable folder or config file line of the side panel; `actions` render outside the button and
// only show on hover unless `pinnedActions` is set
export default function FileListRow({
  name,
  title,
  hint,
  directory = false,
  format,
  active = false,
  actions,
  pinnedActions = false,
  onClick,
}: {
  name: string;
  title?: string;
  /** Muted text after the name, e.g. the folder of a favorite. */
  hint?: string;
  directory?: boolean;
  format: Format | null;
  active?: boolean;
  actions?: ReactNode;
  pinnedActions?: boolean;
  onClick: () => void;
}) {
  const { t: tExt } = useExtTranslations();

  return (
    <div
      className={classNames(
        'group relative flex items-center rounded-md pr-1 transition-colors',
        active
          ? 'bg-(--mantine-primary-color-light) text-(--mantine-primary-color-light-color)'
          : 'hover:bg-(--mantine-color-default-hover)',
      )}
    >
      <button
        type='button'
        title={title ?? name}
        onClick={onClick}
        className='mantine-focus-auto flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1 text-left text-sm!'
      >
        {directory ? (
          <FontAwesomeIcon
            icon={faFolder}
            className='w-4 shrink-0'
            style={{ color: 'var(--mantine-primary-color-filled)' }}
          />
        ) : (
          <FormatIcon format={format} className='w-4 shrink-0' />
        )}
        <span className='min-w-0 truncate'>{name}</span>
        {hint && <span className='min-w-0 flex-1 truncate text-xs text-(--mantine-color-dimmed)'>{hint}</span>}
        {!hint && <span className='flex-1' />}
        {format && (
          <span className='shrink-0 font-mono text-[10px] uppercase tracking-wide text-(--mantine-color-dimmed) group-focus-within:hidden group-hover:hidden'>
            {tExt(`formats.${format}`, {})}
          </span>
        )}
      </button>
      {actions && (
        <div
          className={classNames(
            'flex shrink-0 items-center',
            !pinnedActions && 'hidden group-focus-within:flex group-hover:flex',
          )}
        >
          {actions}
        </div>
      )}
    </div>
  );
}
