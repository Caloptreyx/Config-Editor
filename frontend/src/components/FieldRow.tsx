import type { ReactNode } from 'react';

const HOVER = 'transition-colors hover:bg-white/2 light:hover:bg-black/2';
const ACTIONS =
  'flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100';

// an object entry: key and comment on the left, the value control on the right, actions on hover
export function FieldRow({
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
    <div
      className={`group grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 gap-y-2 px-4 py-3 md:grid-cols-[minmax(10rem,2fr)_minmax(0,3fr)_auto] ${HOVER}`}
    >
      <div className='order-1 flex min-w-0 flex-col gap-1'>
        {label}
        {description && (
          <p
            title={description}
            className='line-clamp-3 whitespace-pre-wrap text-xs text-(--mantine-color-dimmed) leading-snug'
          >
            {description}
          </p>
        )}
      </div>
      <div className='order-3 col-span-2 min-w-0 md:order-2 md:col-span-1'>{children}</div>
      <div className={`${ACTIONS} order-2 justify-end md:order-3`}>{actions}</div>
    </div>
  );
}

// an array item: a compact line with its position, the value control and actions on hover
export function ItemRow({ index, actions, children }: { index: number; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className={`group flex items-center gap-3 px-4 py-1.5 ${HOVER}`}>
      <span className='w-6 shrink-0 text-right font-mono text-xs text-(--mantine-color-dimmed)'>{index}</span>
      <div className='min-w-0 flex-1'>{children}</div>
      <div className={ACTIONS}>{actions}</div>
    </div>
  );
}

// the key of an entry with its type marker
export function FieldName({ name, mark }: { name: string; mark: ReactNode }) {
  return (
    <span className='flex min-w-0 items-center gap-2'>
      <span className='truncate font-mono text-sm font-medium' title={name}>
        {name}
      </span>
      {mark}
    </span>
  );
}
