import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import { configEditorQueryKey, getDirectory } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import DirectoryBreadcrumbs from './DirectoryBreadcrumbs.tsx';
import FavoriteStar from './FavoriteStar.tsx';
import FileListRow from './FileListRow.tsx';

export default function FileBrowser({
  serverUuid,
  activePath,
  isFavorite,
  toggling,
  onOpen,
  onToggleFavorite,
}: {
  serverUuid: string;
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

  return (
    <div className='flex flex-col gap-2'>
      <DirectoryBreadcrumbs directory={directory} onNavigate={setDirectory} />

      {listing.isLoading ? (
        <Spinner.Centered />
      ) : listing.error ? (
        <Alert color='red'>{httpErrorToHuman(listing.error)}</Alert>
      ) : listing.data?.entries.length === 0 ? (
        <p className='px-2 text-xs text-(--mantine-color-dimmed)'>{tExt('browser.empty', {})}</p>
      ) : (
        <div className='flex flex-col'>
          {listing.data?.entries.map((entry) =>
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
          )}
        </div>
      )}
    </div>
  );
}
