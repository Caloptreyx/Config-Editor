import { faSliders } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useComputedColorScheme } from '@mantine/core';
import { useEffect, useMemo, useState } from 'react';
import { NavLink, useBeforeUnload } from 'react-router';
import Button from '@/elements/buttons/Button.tsx';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import { useBlocker } from '@/plugins/useBlocker.ts';
import { useServerCan } from '@/plugins/usePermissions.ts';
import { useTranslations } from '@/providers/TranslationProvider.tsx';
import { useGlobalStore } from '@/stores/global.ts';
import { useServerStore } from '@/stores/server.ts';
import { useExtTranslations } from '../translations.ts';

// Built from workbench/ into frontend/public, which the panel serves from its own origin.
const WORKBENCH_URL = '/config-editor-vscode/index.html';

/**
 * The full VS Code workbench for the server's files. It runs in a same-origin frame because it
 * bundles its own Monaco build; inside, every file operation goes through the panel file API.
 */
export default function VsCodePage() {
  const { t } = useTranslations();
  const { t: tExt } = useExtTranslations();
  const server = useServerStore((state) => state.server);
  const maxContentSearchSize = useGlobalStore((state) => state.settings.server.maxFileManagerContentSearchSize);
  const colorScheme = useComputedColorScheme('dark');
  const canWrite = useServerCan('files.create');
  const canDelete = useServerCan('files.delete');
  const canRename = useServerCan('files.update');
  const [dirty, setDirty] = useState(false);

  // the workbench reads its whole setup once, so changing these reloads it
  const src = useMemo(
    () =>
      `${WORKBENCH_URL}?${new URLSearchParams({
        server: server.uuid,
        name: server.name,
        theme: colorScheme,
        write: canWrite ? '1' : '0',
        delete: canDelete ? '1' : '0',
        rename: canRename ? '1' : '0',
        searchSize: String(maxContentSearchSize),
      })}`,
    [server.uuid, server.name, colorScheme, canWrite, canDelete, canRename, maxContentSearchSize],
  );

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.origin !== window.location.origin) return;
      if (event.data?.type === 'config-editor-vscode:dirty') setDirty(event.data.dirty === true);
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, []);

  const blocker = useBlocker(dirty, true);
  useBeforeUnload((event) => {
    if (dirty) {
      event.preventDefault();
      event.returnValue = '';
    }
  });

  return (
    <ServerContentContainer
      title={tExt('vscode.title', {})}
      subtitle={tExt('vscode.subtitle', {})}
      contentRight={
        <NavLink to={`/server/${server.uuidShort}/config-editor`}>
          <Button size='xs' variant='default' leftSection={<FontAwesomeIcon icon={faSliders} />}>
            {tExt('common.configEditor', {})}
          </Button>
        </NavLink>
      }
    >
      <ConfirmationModal
        title={t('pages.server.files.modal.unsavedChanges.title', {})}
        opened={blocker.state === 'blocked'}
        onClose={blocker.reset}
        onConfirmed={blocker.proceed}
        confirm={t('common.button.leavePage', {})}
      >
        {t('pages.server.files.modal.unsavedChanges.content', {}).md()}
      </ConfirmationModal>

      <iframe
        key={src}
        title={tExt('vscode.title', {})}
        src={src}
        className='block h-[calc(100dvh-12rem)] min-h-[32rem] w-full rounded-md border border-(--mantine-color-default-border)'
      />
    </ServerContentContainer>
  );
}
