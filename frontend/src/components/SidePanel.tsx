import { faMagnifyingGlass, faStar } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useState } from 'react';
import Card from '@/elements/data-display/Card.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import useFavorites from '../pages/useFavorites.ts';
import { useExtTranslations } from '../translations.ts';
import FavoritesList from './FavoritesList.tsx';
import FileBrowser from './FileBrowser.tsx';
import SectionHeader from './SectionHeader.tsx';

export default function SidePanel({
  serverUuid,
  activePath,
  onOpen,
}: {
  serverUuid: string;
  activePath: string | null;
  onOpen: (path: string) => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { favorites, isFavorite, toggleFavorite, toggling } = useFavorites(serverUuid);
  const [filter, setFilter] = useState('');
  const query = filter.trim().toLowerCase();

  return (
    <Card p={0} className='flex min-h-0 flex-col overflow-hidden lg:h-full'>
      <div className='border-b border-(--mantine-color-default-border) p-2'>
        <TextInput
          size='xs'
          placeholder={tExt('browser.filter', {})}
          leftSection={<FontAwesomeIcon icon={faMagnifyingGlass} />}
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
        />
      </div>

      <div className='flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-1.5 pb-3'>
        <div className='flex flex-col gap-1.5'>
          <SectionHeader icon={faStar} label={tExt('browser.favorites', {})} count={favorites.data?.length} />
          <FavoritesList
            favorites={favorites.data ?? []}
            query={query}
            activePath={activePath}
            toggling={toggling}
            onOpen={onOpen}
            onToggleFavorite={toggleFavorite}
          />
        </div>

        <FileBrowser
          serverUuid={serverUuid}
          query={query}
          activePath={activePath}
          isFavorite={isFavorite}
          toggling={toggling}
          onOpen={onOpen}
          onToggleFavorite={toggleFavorite}
        />
      </div>
    </Card>
  );
}
