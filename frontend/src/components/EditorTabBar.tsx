import { faXmark } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import classNames from 'classnames';
import { basename } from 'pathe';
import Spinner from '@/elements/feedback/Spinner.tsx';
import { type EditorTab, isTabDirty } from '../lib/tabs.ts';
import { useExtTranslations } from '../translations.ts';
import { FormatIcon } from './FormatBadge.tsx';

export default function EditorTabBar({
  tabs,
  activePath,
  opening,
  onSelect,
  onClose,
}: {
  tabs: EditorTab[];
  activePath: string | null;
  /** A file is being fetched for a new tab. */
  opening: boolean;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
}) {
  const { t: tExt } = useExtTranslations();

  return (
    <div
      role='tablist'
      className='flex shrink-0 gap-1 overflow-x-auto border-b border-(--mantine-color-default-border) bg-(--mantine-color-default) px-2 pt-2 light:bg-(--mantine-color-gray-0)'
    >
      {tabs.map((tab) => {
        const name = basename(tab.path);
        const active = tab.path === activePath;
        const dirty = isTabDirty(tab);

        return (
          <div
            key={tab.path}
            onAuxClick={(e) => {
              if (e.button !== 1) return;
              e.preventDefault();
              onClose(tab.path);
            }}
            className={classNames(
              'group -mb-px flex h-9 max-w-60 shrink-0 items-center gap-1 rounded-t-md border border-b-0 pr-1.5 transition-colors',
              active
                ? 'border-(--mantine-color-default-border) bg-(--mantine-color-body) text-(--mantine-color-text) dark:bg-(--mantine-color-dark-6)'
                : 'border-transparent text-(--mantine-color-dimmed) hover:bg-(--mantine-color-default-hover) hover:text-(--mantine-color-text)',
            )}
          >
            <button
              type='button'
              role='tab'
              title={tab.path}
              aria-selected={active}
              onClick={() => onSelect(tab.path)}
              className='mantine-focus-auto flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2 pl-3 text-sm!'
            >
              <FormatIcon format={tab.file.format} className='shrink-0 text-xs' />
              <span className={classNames('truncate', dirty && 'italic')}>{name}</span>
            </button>
            <button
              type='button'
              aria-label={tExt('tabs.close', { name })}
              title={dirty ? tExt('tabs.unsaved', { name }) : tExt('tabs.close', { name })}
              onClick={() => onClose(tab.path)}
              className='mantine-focus-auto grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded text-xs! hover:bg-(--mantine-color-default-hover)'
            >
              {dirty && (
                <span className='h-2 w-2 rounded-full bg-(--mantine-primary-color-filled) group-hover:hidden' />
              )}
              <FontAwesomeIcon icon={faXmark} className={classNames(dirty && 'hidden! group-hover:inline-block!')} />
            </button>
          </div>
        );
      })}
      {opening && (
        <div className='flex h-9 items-center px-3'>
          <Spinner size={14} />
        </div>
      )}
    </div>
  );
}
