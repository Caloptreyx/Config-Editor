import { faArrowTurnUp, faFolderTree, faRotateRight } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useQuery } from '@tanstack/react-query';
import { dirname } from 'pathe';
import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import { configEditorQueryKey, getDirectory } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import DirectoryBreadcrumbs from './DirectoryBreadcrumbs.tsx';
import FavoriteStar from './FavoriteStar.tsx';
import FileListRow from './FileListRow.tsx';
import SectionHeader from './SectionHeader.tsx';

export default function FileBrowser({
  serverUuid,
  query,
  activePath,
  isFavorite,
  toggling,
  onOpen,
  onToggleFavorite,
}: {
  serverUuid: string;
  /** Lower-cased name filter. */
  query: string;
  activePath: string | null;
  isFavorite: (path: string) => boolean;
  toggling: boolean;
  onOpen: (path: string) => void;
  onToggleFavorite: (path: string) => void;
}) {
  const { t: tExt } = useExtTranslations();
  const [directory, setDirectory] = useState('/');

  const listing = useQuery({
    queryKey: [...configEditorQueryKey(serverUuid), 'files', directory],
    queryFn: () => getDirectory(serverUuid, directory),
  });
  const entries = (listing.data?.entries ?? []).filter((entry) => entry.name.toLowerCase().includes(query));

  return (
    <div className='flex flex-col gap-1.5'>
      <SectionHeader
        icon={faFolderTree}
        label={tExt('browser.files', {})}
        actions={
          <Tooltip label={tExt('browser.refresh', {})}>
            <ActionIcon
              size='xs'
              variant='subtle'
              color='gray'
              aria-label={tExt('browser.refresh', {})}
              loading={listing.isFetching}
              onClick={() => listing.refetch()}
            >
              <FontAwesomeIcon icon={faRotateRight} />
            </ActionIcon>
          </Tooltip>
        }
      />
      <DirectoryBreadcrumbs directory={directory} onNavigate={setDirectory} />

      <div className='flex flex-col gap-px'>
        {directory !== '/' && (
          <button
            type='button'
            className='mantine-focus-auto flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1 text-sm! text-(--mantine-color-dimmed) hover:bg-(--mantine-color-default-hover)'
            onClick={() => setDirectory(dirname(directory))}
          >
            <FontAwesomeIcon icon={faArrowTurnUp} className='w-4 shrink-0 -scale-x-100' />
            {tExt('browser.up', {})}
          </button>
        )}

        {listing.isLoading ? (
          <Spinner.Centered />
        ) : listing.error ? (
          <Alert color='red'>{httpErrorToHuman(listing.error)}</Alert>
        ) : entries.length === 0 ? (
          <p className='px-2.5 py-1 text-xs text-(--mantine-color-dimmed)'>
            {query ? tExt('browser.noMatches', {}) : tExt('browser.empty', {})}
          </p>
        ) : (
          entries.map((entry) =>
            entry.directory ? (
              <FileListRow
                key={entry.path}
                name={entry.name}
                directory
                format={null}
                onClick={() => setDirectory(entry.path)}
              />
            ) : (
              <FileListRow
                key={entry.path}
                name={entry.name}
                format={entry.format}
                active={entry.path === activePath}
                pinnedActions={isFavorite(entry.path)}
                onClick={() => onOpen(entry.path)}
                actions={
                  <FavoriteStar
                    favorite={isFavorite(entry.path)}
                    disabled={toggling}
                    onToggle={() => onToggleFavorite(entry.path)}
                  />
                }
              />
            ),
          )
        )}
      </div>
    </div>
  );
}
