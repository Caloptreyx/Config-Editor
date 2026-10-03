import { faStar } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { basename, dirname } from 'pathe';
import type { Favorite } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import FavoriteStar from './FavoriteStar.tsx';
import FileListRow from './FileListRow.tsx';

export default function FavoritesList({
  favorites,
  query,
  activePath,
  toggling,
  onOpen,
  onToggleFavorite,
}: {
  favorites: Favorite[];
  /** Lower-cased name filter. */
  query: string;
  activePath: string | null;
  toggling: boolean;
  onOpen: (path: string) => void;
  onToggleFavorite: (path: string) => void;
}) {
  const { t: tExt } = useExtTranslations();

  if (favorites.length === 0) {
    return (
      <div className='mx-1 flex items-center gap-2.5 rounded-md border border-dashed border-(--mantine-color-default-border) px-3 py-2.5 text-xs text-(--mantine-color-dimmed)'>
        <FontAwesomeIcon icon={faStar} className='shrink-0' />
        {tExt('browser.noFavorites', {})}
      </div>
    );
  }

  const shown = favorites.filter((favorite) => basename(favorite.path).toLowerCase().includes(query));
  if (shown.length === 0) {
    return <p className='px-2.5 text-xs text-(--mantine-color-dimmed)'>{tExt('browser.noMatches', {})}</p>;
  }

  return (
    <div className='flex flex-col gap-px'>
      {shown.map((favorite) => {
        const folder = dirname(favorite.path);

        return (
          <FileListRow
            key={favorite.path}
            name={basename(favorite.path)}
            title={favorite.path}
            hint={folder === '/' ? undefined : folder.replace(/^\//, '')}
            format={favorite.format}
            active={favorite.path === activePath}
            onClick={() => onOpen(favorite.path)}
            actions={<FavoriteStar favorite disabled={toggling} onToggle={() => onToggleFavorite(favorite.path)} />}
          />
        );
      })}
    </div>
  );
}
