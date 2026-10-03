import { faChevronRight, faHouse } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import classNames from 'classnames';
import { join } from 'pathe';
import { Fragment } from 'react';
import { pathSegments } from '@/lib/path.ts';
import { useExtTranslations } from '../translations.ts';

const CRUMB_CLASS =
  'mantine-focus-auto min-w-0 cursor-pointer truncate rounded px-1 py-0.5 hover:bg-(--mantine-color-default-hover)';

export default function DirectoryBreadcrumbs({
  directory,
  onNavigate,
}: {
  directory: string;
  onNavigate: (directory: string) => void;
}) {
  const { t: tExt } = useExtTranslations();
  const segments = pathSegments(directory);

  return (
    <div className='mx-1 flex min-w-0 flex-wrap items-center gap-0.5 rounded-md bg-(--mantine-color-default) px-1.5 py-1 font-mono text-xs light:bg-(--mantine-color-gray-0)'>
      <button
        type='button'
        title={tExt('browser.root', {})}
        aria-label={tExt('browser.root', {})}
        className={classNames(
          CRUMB_CLASS,
          segments.length === 0 ? 'text-(--mantine-color-text)' : 'text-(--mantine-color-dimmed)',
        )}
        onClick={() => onNavigate('/')}
      >
        <FontAwesomeIcon icon={faHouse} />
      </button>
      {segments.map((segment, index) => (
        <Fragment key={segment.path}>
          <FontAwesomeIcon icon={faChevronRight} className='text-[8px] text-(--mantine-color-dimmed)' />
          <button
            type='button'
            title={segment.name}
            className={classNames(
              CRUMB_CLASS,
              index === segments.length - 1 ? 'text-(--mantine-color-text)' : 'text-(--mantine-color-dimmed)',
            )}
            onClick={() => onNavigate(join('/', segment.path))}
          >
            {segment.name}
          </button>
        </Fragment>
      ))}
    </div>
  );
}
