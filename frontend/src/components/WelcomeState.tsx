import {
  faCodeCompare,
  faCommentDots,
  faKeyboard,
  faSliders,
  type IconDefinition,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { FORMATS } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import FormatBadge from './FormatBadge.tsx';

function Hint({ icon, title, text }: { icon: IconDefinition; title: string; text: string }) {
  return (
    <div className='flex items-start gap-3 rounded-md border border-(--mantine-color-default-border) p-3 text-left'>
      <FontAwesomeIcon icon={icon} className='mt-0.5 w-4 shrink-0 text-(--mantine-primary-color-light-color)' />
      <div className='flex flex-col gap-0.5'>
        <span className='text-sm font-medium'>{title}</span>
        <span className='text-xs text-(--mantine-color-dimmed) leading-snug'>{text}</span>
      </div>
    </div>
  );
}

// shown in the editor panel while no file is open
export default function WelcomeState() {
  const { t: tExt } = useExtTranslations();

  return (
    <div className='flex flex-1 items-center justify-center p-8'>
      <div className='flex max-w-2xl flex-col items-center gap-5 text-center'>
        <div className='grid h-14 w-14 place-items-center rounded-xl bg-(--mantine-primary-color-light) text-2xl text-(--mantine-primary-color-light-color)'>
          <FontAwesomeIcon icon={faSliders} />
        </div>
        <div className='flex flex-col gap-1.5'>
          <span className='text-lg font-semibold'>{tExt('welcome.title', {})}</span>
          <span className='text-sm text-(--mantine-color-dimmed)'>{tExt('welcome.description', {})}</span>
        </div>
        <div className='flex flex-wrap justify-center gap-1.5'>
          {FORMATS.map((format) => (
            <FormatBadge key={format} format={format} size='sm' />
          ))}
        </div>
        <div className='grid w-full gap-2 sm:grid-cols-3'>
          <Hint icon={faCommentDots} title={tExt('welcome.commentsTitle', {})} text={tExt('welcome.comments', {})} />
          <Hint icon={faCodeCompare} title={tExt('welcome.reviewTitle', {})} text={tExt('welcome.review', {})} />
          <Hint icon={faKeyboard} title={tExt('welcome.tabsTitle', {})} text={tExt('welcome.tabs', {})} />
        </div>
      </div>
    </div>
  );
}
