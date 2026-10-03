import { faPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { type ComponentPropsWithoutRef, forwardRef } from 'react';

// the dashed "add" line at the end of an object or list; forwards its ref so it can be a menu/popover target
const AddButton = forwardRef<HTMLButtonElement, ComponentPropsWithoutRef<'button'> & { label: string }>(
  ({ label, ...rest }, ref) => (
    <button
      type='button'
      ref={ref}
      {...rest}
      className='mantine-focus-auto flex w-full cursor-pointer items-center justify-center gap-2 rounded-md border border-dashed border-(--mantine-color-dimmed)/35 px-3 py-1.5 text-xs! text-(--mantine-color-dimmed) transition-colors hover:border-(--mantine-primary-color-filled) hover:text-(--mantine-primary-color-light-color)'
    >
      <FontAwesomeIcon icon={faPlus} />
      {label}
    </button>
  ),
);

export default AddButton;
