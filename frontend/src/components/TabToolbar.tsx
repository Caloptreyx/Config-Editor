import {
  faArrowRotateLeft,
  faArrowUpRightFromSquare,
  faCodeCompare,
  faFloppyDisk,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { NavLink } from 'react-router';
import Button from '@/elements/buttons/Button.tsx';
import Switch from '@/elements/input/Switch.tsx';
import Group from '@/elements/layout/Group.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useExtTranslations } from '../translations.ts';

export default function TabToolbar({
  path,
  raw,
  rawLockedReason,
  dirty,
  savable,
  invalid,
  canSave,
  saving,
  reviewing,
  fileEditorUrl,
  onSave,
  onRevert,
  onReview,
  onToggleRaw,
}: {
  path: string;
  raw: boolean;
  /** Why the raw/visual toggle is disabled, or null when it can be used. */
  rawLockedReason: string | null;
  dirty: boolean;
  savable: boolean;
  invalid: boolean;
  canSave: boolean;
  saving: boolean;
  reviewing: boolean;
  fileEditorUrl: string;
  onSave: () => void;
  onRevert: () => void;
  onReview: () => void;
  onToggleRaw: () => void;
}) {
  const { t: tExt } = useExtTranslations();

  return (
    <div className='flex flex-col gap-2'>
      <Group justify='space-between' gap='sm'>
        <Text size='sm' c='dimmed' className='min-w-0 truncate font-mono' title={path}>
          {path}
        </Text>
        <Group gap='sm'>
          <Tooltip label={rawLockedReason} disabled={rawLockedReason === null}>
            <Switch
              label={tExt('toolbar.rawMode', {})}
              checked={raw}
              disabled={rawLockedReason !== null}
              onChange={onToggleRaw}
            />
          </Tooltip>
          <NavLink to={fileEditorUrl}>
            <Button variant='subtle' size='xs' leftSection={<FontAwesomeIcon icon={faArrowUpRightFromSquare} />}>
              {tExt('toolbar.openInFileEditor', {})}
            </Button>
          </NavLink>
        </Group>
      </Group>

      {canSave && (
        <Group gap='sm'>
          <Button
            size='xs'
            leftSection={<FontAwesomeIcon icon={faFloppyDisk} />}
            disabled={!savable}
            loading={saving}
            onClick={onSave}
          >
            {tExt('toolbar.save', {})}
          </Button>
          <Button
            size='xs'
            variant='default'
            leftSection={<FontAwesomeIcon icon={faArrowRotateLeft} />}
            disabled={!dirty || saving}
            onClick={onRevert}
          >
            {tExt('toolbar.revert', {})}
          </Button>
          <Button
            size='xs'
            variant='default'
            leftSection={<FontAwesomeIcon icon={faCodeCompare} />}
            disabled={!dirty || invalid}
            loading={reviewing}
            onClick={onReview}
          >
            {tExt('toolbar.review', {})}
          </Button>
          {invalid && (
            <Text size='xs' c='red'>
              {tExt('toolbar.invalidValues', {})}
            </Text>
          )}
        </Group>
      )}
    </div>
  );
}
