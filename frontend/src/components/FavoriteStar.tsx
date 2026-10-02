import { faStar } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import { useExtTranslations } from '../translations.ts';

export default function FavoriteStar({
  favorite,
  disabled,
  onToggle,
}: {
  favorite: boolean;
  disabled?: boolean;
  onToggle: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const label = favorite ? tExt('browser.removeFavorite', {}) : tExt('browser.addFavorite', {});

  return (
    <Tooltip label={label}>
      <ActionIcon
        size='sm'
        variant='subtle'
        color={favorite ? 'yellow' : 'gray'}
        aria-label={label}
        disabled={disabled}
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
      >
        <FontAwesomeIcon icon={faStar} />
      </ActionIcon>
    </Tooltip>
  );
}
