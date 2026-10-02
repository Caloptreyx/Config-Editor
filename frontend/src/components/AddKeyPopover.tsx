import { faPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { type FormEvent, useState } from 'react';
import Button from '@/elements/buttons/Button.tsx';
import Select from '@/elements/input/Select.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Popover from '@/elements/overlays/Popover.tsx';
import { useTranslations } from '@/providers/TranslationProvider.tsx';
import { appendEntry, defaultNode, type NodeKind, type NodePath } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';
import { useDocumentEditing } from './documentEditing.ts';

export default function AddKeyPopover({
  path,
  kinds,
  existingKeys,
}: {
  path: NodePath;
  kinds: NodeKind[];
  existingKeys: string[];
}) {
  const { t } = useTranslations();
  const { t: tExt } = useExtTranslations();
  const { edit } = useDocumentEditing();

  const [opened, setOpened] = useState(false);
  const [key, setKey] = useState('');
  const [kind, setKind] = useState<NodeKind>(kinds[0]);
  const [error, setError] = useState<string | null>(null);

  const close = () => {
    setOpened(false);
    setKey('');
    setError(null);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    e.stopPropagation();

    if (!key.trim()) {
      setError(tExt('editor.keyRequired', {}));
      return;
    }
    if (existingKeys.includes(key)) {
      setError(tExt('editor.keyDuplicate', {}));
      return;
    }

    edit((document) => appendEntry(document, path, key, defaultNode(kind)));
    close();
  };

  return (
    <Popover opened={opened} onChange={(next) => (next ? setOpened(true) : close())} position='bottom-start' trapFocus>
      <Popover.Target>
        <Button
          size='xs'
          variant='light'
          className='self-start'
          leftSection={<FontAwesomeIcon icon={faPlus} />}
          onClick={() => (opened ? close() : setOpened(true))}
        >
          {tExt('editor.addKey', {})}
        </Button>
      </Popover.Target>
      <Popover.Dropdown>
        <form onSubmit={submit}>
          <Stack gap='xs' w={260}>
            <TextInput
              label={tExt('editor.keyName', {})}
              value={key}
              error={error}
              data-autofocus
              onChange={(e) => {
                setKey(e.currentTarget.value);
                setError(null);
              }}
            />
            <Select
              label={tExt('editor.kind', {})}
              value={kind}
              data={kinds.map((option) => ({ value: option, label: tExt(`kinds.${option}`, {}) }))}
              onChange={(value) => setKind(kinds.find((option) => option === value) ?? kind)}
              comboboxProps={{ withinPortal: false }}
            />
            <Button type='submit' size='xs'>
              {t('common.button.add', {})}
            </Button>
          </Stack>
        </form>
      </Popover.Dropdown>
    </Popover>
  );
}
