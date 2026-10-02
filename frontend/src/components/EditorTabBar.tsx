import { faXmark } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import classNames from 'classnames';
import { basename } from 'pathe';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import UnstyledButton from '@/elements/buttons/UnstyledButton.tsx';
import { type EditorTab, isTabDirty } from '../lib/tabs.ts';
import { useExtTranslations } from '../translations.ts';
import FormatBadge from './FormatBadge.tsx';

export default function EditorTabBar({
  tabs,
  activePath,
  onSelect,
  onClose,
}: {
  tabs: EditorTab[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
}) {
  const { t: tExt } = useExtTranslations();

  return (
    <div role='tablist' className='flex overflow-x-auto border-b border-(--mantine-color-default-border)'>
      {tabs.map((tab) => {
        const name = basename(tab.path);
        const active = tab.path === activePath;

        return (
          <div
            key={tab.path}
            onAuxClick={(e) => {
              if (e.button !== 1) return;
              e.preventDefault();
              onClose(tab.path);
            }}
            className={classNames(
              'flex h-10.5 max-w-64 shrink-0 items-center gap-1 border-r border-b-2 border-(--mantine-color-default-border)',
              active
                ? 'border-b-(--mantine-primary-color-filled) bg-(--mantine-color-default-hover)'
                : 'border-b-transparent hover:bg-(--mantine-color-default-hover)',
            )}
          >
            <UnstyledButton
              role='tab'
              title={tab.path}
              aria-selected={active}
              onClick={() => onSelect(tab.path)}
              className='flex min-w-0 flex-1 items-center gap-2 py-2 pl-3 text-sm'
            >
              <span className='truncate'>{name}</span>
              <FormatBadge format={tab.file.format} />
              {isTabDirty(tab) && (
                <span
                  aria-label={tExt('tabs.unsaved', { name })}
                  className='h-2 w-2 shrink-0 rounded-full bg-(--mantine-primary-color-filled)'
                />
              )}
            </UnstyledButton>
            <ActionIcon
              size='xs'
              variant='subtle'
              color='gray'
              className='mr-1.5 shrink-0'
              aria-label={tExt('tabs.close', { name })}
              onClick={() => onClose(tab.path)}
            >
              <FontAwesomeIcon icon={faXmark} />
            </ActionIcon>
          </div>
        );
      })}
    </div>
  );
}
