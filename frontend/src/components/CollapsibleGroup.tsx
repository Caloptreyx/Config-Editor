import { faChevronRight } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { type ReactNode, useState } from 'react';
import UnstyledButton from '@/elements/buttons/UnstyledButton.tsx';
import Collapse from '@/elements/layout/Collapse.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useExtTranslations } from '../translations.ts';

// an object or array value: a header that toggles its indented children
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
    <div className='rounded-md border border-(--mantine-color-default-border)'>
      <div className='flex items-center gap-2 px-2 py-1'>
        <UnstyledButton
          aria-expanded={expanded}
          aria-label={expanded ? tExt('editor.collapse', {}) : tExt('editor.expand', {})}
          className='flex min-w-0 flex-1 items-center gap-2 py-1 text-sm font-medium'
          onClick={() => setOpen(!expanded)}
        >
          <FontAwesomeIcon
            icon={faChevronRight}
            className='w-3 shrink-0 transition-transform'
            style={{ transform: expanded ? 'rotate(90deg)' : undefined }}
          />
          {label}
          <Text component='span' size='xs' c='dimmed' className='shrink-0'>
            {summary}
          </Text>
        </UnstyledButton>
        {actions}
      </div>
      {description && (
        <Text size='xs' c='dimmed' px='sm' pb={4} className='whitespace-pre-wrap'>
          {description}
        </Text>
      )}
      <Collapse expanded={expanded} keepMounted={false}>
        <div className='ml-3 border-l-2 border-(--mantine-color-default-border) py-2 pr-2 pl-3'>{children}</div>
      </Collapse>
    </div>
  );
}
