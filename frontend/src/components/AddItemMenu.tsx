import { faPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import Button from '@/elements/buttons/Button.tsx';
import Menu from '@/elements/overlays/Menu.tsx';
import { appendItem, defaultNode, type NodeKind, type NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import { useDocumentEditing } from './documentEditing.ts';

export default function AddItemMenu({ path, kinds }: { path: NodePath; kinds: NodeKind[] }) {
  const { t: tExt } = useExtTranslations();
  const { edit } = useDocumentEditing();

  return (
    <Menu position='bottom-start'>
      <Menu.Target>
        <Button size='xs' variant='light' className='self-start' leftSection={<FontAwesomeIcon icon={faPlus} />}>
          {tExt('editor.addItem', {})}
        </Button>
      </Menu.Target>
      <Menu.Dropdown>
        {kinds.map((kind) => (
          <Menu.Item key={kind} onClick={() => edit((document) => appendItem(document, path, defaultNode(kind)))}>
            {tExt(`kinds.${kind}`, {})}
          </Menu.Item>
        ))}
      </Menu.Dropdown>
    </Menu>
  );
}
