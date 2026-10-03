import { faFileLines } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import Badge from '@/elements/data-display/Badge.tsx';
import type { Format } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';

const FORMAT_COLORS: Record<Format, string> = {
  yaml: 'grape',
  json: 'yellow',
  toml: 'orange',
  properties: 'blue',
  env: 'teal',
  ini: 'cyan',
};

// text color of a format, adapting to the color scheme
export const formatColor = (format: Format) => `var(--mantine-color-${FORMAT_COLORS[format]}-light-color)`;

export function FormatIcon({ format, className }: { format: Format | null; className?: string }) {
  return (
    <FontAwesomeIcon
      icon={faFileLines}
      className={className}
      style={{ color: format ? formatColor(format) : 'var(--mantine-color-dimmed)' }}
    />
  );
}

export default function FormatBadge({ format, size = 'xs' }: { format: Format; size?: 'xs' | 'sm' }) {
  const { t: tExt } = useExtTranslations();

  return (
    <Badge size={size} variant='light' color={FORMAT_COLORS[format]} className='shrink-0'>
      {tExt(`formats.${format}`, {})}
    </Badge>
  );
}
