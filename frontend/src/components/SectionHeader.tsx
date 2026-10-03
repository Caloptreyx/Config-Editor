import type { IconDefinition } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import type { ReactNode } from 'react';

// small uppercase heading of a side panel section with an optional count and actions
export default function SectionHeader({
  icon,
  label,
  count,
  actions,
}: {
  icon: IconDefinition;
  label: string;
  count?: number;
  actions?: ReactNode;
}) {
  return (
    <div className='flex h-7 items-center gap-2 px-2.5 text-[11px] font-semibold uppercase tracking-wider text-(--mantine-color-dimmed)'>
      <FontAwesomeIcon icon={icon} className='w-3' />
      <span>{label}</span>
      {count !== undefined && count > 0 && (
        <span className='rounded-full bg-(--mantine-color-default-hover) px-1.5 py-px text-[10px] leading-4'>
          {count}
        </span>
      )}
      <span className='flex-1' />
      {actions}
    </div>
  );
}
