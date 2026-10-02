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

export default function FormatBadge({ format }: { format: Format }) {
  const { t: tExt } = useExtTranslations();

  return (
    <Badge size='xs' variant='light' color={FORMAT_COLORS[format]} className='shrink-0'>
      {tExt(`formats.${format}`, {})}
    </Badge>
  );
}
