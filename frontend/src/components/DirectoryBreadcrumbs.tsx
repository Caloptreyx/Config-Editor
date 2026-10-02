import { join } from 'pathe';
import UnstyledButton from '@/elements/buttons/UnstyledButton.tsx';
import Breadcrumbs from '@/elements/data-display/Breadcrumbs.tsx';
import { pathSegments } from '@/lib/path.ts';
import { useExtTranslations } from '../translations.ts';

const LINK_CLASS = 'text-sm text-(--mantine-color-anchor) hover:underline';

export default function DirectoryBreadcrumbs({
  directory,
  onNavigate,
}: {
  directory: string;
  onNavigate: (directory: string) => void;
}) {
  const { t: tExt } = useExtTranslations();
  const segments = pathSegments(directory);

  return (
    <Breadcrumbs separatorMargin={4} className='flex-wrap px-2'>
      <UnstyledButton className={LINK_CLASS} onClick={() => onNavigate('/')}>
        {tExt('browser.root', {})}
      </UnstyledButton>
      {segments.map((segment, index) =>
        index === segments.length - 1 ? (
          <span key={segment.path} className='text-sm'>
            {segment.name}
          </span>
        ) : (
          <UnstyledButton key={segment.path} className={LINK_CLASS} onClick={() => onNavigate(join('/', segment.path))}>
            {segment.name}
          </UnstyledButton>
        ),
      )}
    </Breadcrumbs>
  );
}
