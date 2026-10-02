import type { ReactNode } from 'react';
import Text from '@/elements/typography/Text.tsx';

// label line (with row actions on the right), the entry comment and the value control
export default function FieldRow({
  label,
  description,
  actions,
  children,
}: {
  label: ReactNode;
  description?: string | null;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className='flex flex-col gap-1'>
      <div className='flex min-h-7 items-center justify-between gap-2 text-sm font-medium'>
        {label}
        {actions}
      </div>
      {description && (
        <Text size='xs' c='dimmed' className='whitespace-pre-wrap'>
          {description}
        </Text>
      )}
      {children}
    </div>
  );
}
