import { faChevronRight } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { type ReactNode, useState } from 'react';
import Collapse from '@/elements/layout/Collapse.tsx';
import { useExtTranslations } from '../translations.ts';

// an object or array value: a card whose header toggles the rows inside it
export default function CollapsibleGroup({
  label,
  summary,
  description,
  actions,
  defaultOpen,
  forceOpen,
  children,
}: {
  label: ReactNode;
  summary: string;
  description?: string | null;
  actions?: ReactNode;
  defaultOpen: boolean;
  forceOpen: boolean;
  children: ReactNode;
}) {
  const { t: tExt } = useExtTranslations();
  const [open, setOpen] = useState(defaultOpen);
  const expanded = open || forceOpen;

  return (
    <div className='overflow-hidden rounded-md border border-(--mantine-color-default-border)'>
      <div className='group flex items-start gap-2 bg-(--mantine-color-default) px-3 py-2 light:bg-(--mantine-color-gray-0)'>
        <button
          type='button'
          aria-expanded={expanded}
          aria-label={expanded ? tExt('editor.collapse', {}) : tExt('editor.expand', {})}
          className='mantine-focus-auto flex min-w-0 flex-1 cursor-pointer flex-col gap-1 text-left'
          onClick={() => setOpen(!expanded)}
        >
          <span className='flex min-h-7 min-w-0 items-center gap-2.5'>
            <FontAwesomeIcon
              icon={faChevronRight}
              className='w-3 shrink-0 text-xs text-(--mantine-color-dimmed) transition-transform'
              style={{ transform: expanded ? 'rotate(90deg)' : undefined }}
            />
            {label}
            <span className='shrink-0 text-xs text-(--mantine-color-dimmed)'>{summary}</span>
          </span>
          {description && (
            <span
              title={description}
              className='line-clamp-2 whitespace-pre-wrap pl-5.5 text-xs text-(--mantine-color-dimmed) leading-snug'
            >
              {description}
            </span>
          )}
        </button>
        <div className='flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100'>
          {actions}
        </div>
      </div>
      <Collapse expanded={expanded} keepMounted={false}>
        <div className='border-t border-(--mantine-color-default-border)'>{children}</div>
      </Collapse>
    </div>
  );
}
