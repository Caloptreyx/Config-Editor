import Card from '@/elements/data-display/Card.tsx';
import Divider from '@/elements/layout/Divider.tsx';
import Text from '@/elements/typography/Text.tsx';
import useFavorites from '../pages/useFavorites.ts';
import { useExtTranslations } from '../translations.ts';
import FavoritesList from './FavoritesList.tsx';
import FileBrowser from './FileBrowser.tsx';

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

  return (
    <Card p='xs' className='lg:sticky lg:top-4'>
      <div className='lg:max-h-[calc(100vh-8rem)] overflow-y-auto'>
        <Text size='xs' fw={600} tt='uppercase' c='dimmed' px='xs' py={4}>
          {tExt('browser.favorites', {})}
        </Text>
        <FavoritesList
          favorites={favorites.data ?? []}
          activePath={activePath}
          toggling={toggling}
          onOpen={onOpen}
          onToggleFavorite={toggleFavorite}
        />

        <Divider my='xs' />

        <Text size='xs' fw={600} tt='uppercase' c='dimmed' px='xs' py={4}>
          {tExt('browser.files', {})}
        </Text>
        <FileBrowser
          serverUuid={serverUuid}
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
