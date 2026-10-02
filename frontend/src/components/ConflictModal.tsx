import Button from '@/elements/buttons/Button.tsx';
import { Modal, ModalFooter } from '@/elements/modals/Modal.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useTranslations } from '@/providers/TranslationProvider.tsx';
import { useExtTranslations } from '../translations.ts';

export default function ConflictModal({
  name,
  loading,
  onReload,
  onOverwrite,
  onClose,
}: {
  name: string | null;
  loading: boolean;
  onReload: () => void;
  onOverwrite: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslations();
  const { t: tExt } = useExtTranslations();

  return (
    <Modal title={tExt('conflict.title', {})} opened={name !== null} onClose={onClose}>
      <Text>{tExt('conflict.content', { name: name ?? '' })}</Text>

      <ModalFooter>
        <Button color='red' loading={loading} onClick={onOverwrite}>
          {tExt('conflict.overwrite', {})}
        </Button>
        <Button variant='default' disabled={loading} onClick={onReload}>
          {tExt('conflict.reload', {})}
        </Button>
        <Button variant='default' disabled={loading} onClick={onClose}>
          {t('common.button.cancel', {})}
        </Button>
      </ModalFooter>
    </Modal>
  );
}
