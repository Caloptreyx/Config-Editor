import {
  faArrowRotateLeft,
  faArrowUpRightFromSquare,
  faCircle,
  faCircleCheck,
  faCircleExclamation,
  faCodeCompare,
  faFloppyDisk,
  faLock,
  type IconDefinition,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { basename } from 'pathe';
import { NavLink } from 'react-router';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import SegmentedControl from '@/elements/layout/SegmentedControl.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import type { Format } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import FormatBadge, { FormatIcon, formatColor } from './FormatBadge.tsx';

function Status({ icon, color, label }: { icon: IconDefinition; color: string; label: string }) {
  return (
    <span className='inline-flex shrink-0 items-center gap-1.5 text-xs' style={{ color }}>
      <FontAwesomeIcon icon={icon} className={icon === faCircle ? 'text-[7px]' : undefined} />
      {label}
    </span>
  );
}

// the header of an open file: name, path and state on the left, editor mode and save actions on the right
export default function TabToolbar({
  path,
  format,
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
  format: Format;
  raw: boolean;
  /** Why the form/raw switch is disabled, or null when it can be used. */
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

  const status = !canSave ? (
    <Status icon={faLock} color='var(--mantine-color-dimmed)' label={tExt('toolbar.readOnly', {})} />
  ) : invalid ? (
    <Status
      icon={faCircleExclamation}
      color='var(--mantine-color-red-light-color)'
      label={tExt('toolbar.invalidValues', {})}
    />
  ) : dirty ? (
    <Status icon={faCircle} color='var(--mantine-color-orange-light-color)' label={tExt('toolbar.unsaved', {})} />
  ) : (
    <Status icon={faCircleCheck} color='var(--mantine-color-green-light-color)' label={tExt('toolbar.upToDate', {})} />
  );

  return (
    <div className='flex shrink-0 flex-wrap items-center gap-x-4 gap-y-3 border-b border-(--mantine-color-default-border) px-4 py-3'>
      <div className='flex min-w-0 flex-1 items-center gap-3'>
        <div
          className='grid h-10 w-10 shrink-0 place-items-center rounded-md text-lg'
          style={{ backgroundColor: 'var(--mantine-color-default-hover)', color: formatColor(format) }}
        >
          <FormatIcon format={format} />
        </div>
        <div className='flex min-w-0 flex-col gap-0.5'>
          <div className='flex min-w-0 items-center gap-2'>
            <span className='truncate font-semibold'>{basename(path)}</span>
            <FormatBadge format={format} />
            {status}
          </div>
          <span className='truncate font-mono text-xs text-(--mantine-color-dimmed)' title={path}>
            {path}
          </span>
        </div>
      </div>

      <div className='flex flex-wrap items-center gap-2'>
        <Tooltip label={rawLockedReason} disabled={rawLockedReason === null}>
          <div>
            <SegmentedControl
              size='xs'
              value={raw ? 'raw' : 'form'}
              disabled={rawLockedReason !== null}
              onChange={(value) => {
                if ((value === 'raw') !== raw) onToggleRaw();
              }}
              data={[
                { value: 'form', label: tExt('toolbar.formMode', {}) },
                { value: 'raw', label: tExt('toolbar.rawMode', {}) },
              ]}
            />
          </div>
        </Tooltip>

        <Tooltip label={tExt('toolbar.openInFileEditor', {})}>
          <NavLink to={fileEditorUrl} aria-label={tExt('toolbar.openInFileEditor', {})}>
            <ActionIcon variant='default' size='md' tabIndex={-1}>
              <FontAwesomeIcon icon={faArrowUpRightFromSquare} />
            </ActionIcon>
          </NavLink>
        </Tooltip>

        {canSave && (
          <>
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
            <Tooltip label={tExt('toolbar.saveShortcut', {})}>
              <Button
                size='xs'
                leftSection={<FontAwesomeIcon icon={faFloppyDisk} />}
                disabled={!savable}
                loading={saving}
                onClick={onSave}
              >
                {tExt('toolbar.save', {})}
              </Button>
            </Tooltip>
          </>
        )}
      </div>
    </div>
  );
}
