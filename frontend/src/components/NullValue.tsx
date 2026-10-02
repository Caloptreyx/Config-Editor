import { faPen } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Group from '@/elements/layout/Group.tsx';
import Menu from '@/elements/overlays/Menu.tsx';
import { allowedChildKinds } from '../lib/formats.ts';
import { defaultNode, type NodePath, updateAt } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import { useDocumentEditing } from './documentEditing.ts';

export default function NullValue({ path }: { path: NodePath }) {
  const { t: tExt } = useExtTranslations();
  const { format, readOnly, edit } = useDocumentEditing();
  const kinds = allowedChildKinds(format, path.slice(0, -1)).filter((kind) => kind !== 'null');

  return (
    <Group gap='xs'>
      <Badge variant='light' color='gray' className='font-mono!'>
        null
      </Badge>
      {!readOnly && kinds.length > 0 && (
        <Menu position='bottom-start'>
          <Menu.Target>
            <Button size='compact-xs' variant='subtle' leftSection={<FontAwesomeIcon icon={faPen} />}>
              {tExt('editor.setValue', {})}
            </Button>
          </Menu.Target>
          <Menu.Dropdown>
            {kinds.map((kind) => (
              <Menu.Item
                key={kind}
                onClick={() => edit((document) => updateAt(document, path, () => defaultNode(kind)))}
              >
                {tExt(`kinds.${kind}`, {})}
              </Menu.Item>
            ))}
          </Menu.Dropdown>
        </Menu>
      )}
    </Group>
  );
}
