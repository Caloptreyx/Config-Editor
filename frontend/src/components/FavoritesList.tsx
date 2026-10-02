import { basename } from 'pathe';
import type { Favorite } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import FavoriteStar from './FavoriteStar.tsx';
import FileListRow from './FileListRow.tsx';

export default function FavoritesList({
  favorites,
  activePath,
  toggling,
  onOpen,
  onToggleFavorite,
}: {
  favorites: Favorite[];
  activePath: string | null;
  toggling: boolean;
  onOpen: (path: string) => void;
  onToggleFavorite: (path: string) => void;
}) {
  const { t: tExt } = useExtTranslations();

  if (favorites.length === 0) {
    return <p className='px-2 text-xs text-(--mantine-color-dimmed)'>{tExt('browser.noFavorites', {})}</p>;
  }

  return (
    <div className='flex flex-col'>
      {favorites.map((favorite) => (
        <FileListRow
          key={favorite.path}
          name={basename(favorite.path)}
          title={favorite.path}
          format={favorite.format}
          active={favorite.path === activePath}
          onClick={() => onOpen(favorite.path)}
          actions={<FavoriteStar favorite disabled={toggling} onToggle={() => onToggleFavorite(favorite.path)} />}
        />
      ))}
    </div>
  );
}
