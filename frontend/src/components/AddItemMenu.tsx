import Menu from '@/elements/overlays/Menu.tsx';
import { appendItem, defaultNode, type NodeKind, type NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import AddButton from './AddButton.tsx';
import { useDocumentEditing } from './documentEditing.ts';

export default function AddItemMenu({ path, kinds }: { path: NodePath; kinds: NodeKind[] }) {
  const { t: tExt } = useExtTranslations();
  const { edit } = useDocumentEditing();

  return (
    <Menu position='bottom'>
      <Menu.Target>
        <AddButton label={tExt('editor.addItem', {})} />
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
