import { basename } from 'pathe';
import { useState } from 'react';
import { useBeforeUnload } from 'react-router';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import Card from '@/elements/data-display/Card.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import { useBlocker } from '@/plugins/useBlocker.ts';
import { useServerCan } from '@/plugins/usePermissions.ts';
import { useTranslations } from '@/providers/TranslationProvider.tsx';
import { useServerStore } from '@/stores/server.ts';
import ConflictModal from '../components/ConflictModal.tsx';
import EditorTabBar from '../components/EditorTabBar.tsx';
import SidePanel from '../components/SidePanel.tsx';
import TabEditor from '../components/TabEditor.tsx';
import WelcomeState from '../components/WelcomeState.tsx';
import { isTabDirty, isTabSavable } from '../lib/tabs.ts';
import { useExtTranslations } from '../translations.ts';
import useEditorTabs from './useEditorTabs.ts';
import useSaveShortcut from './useSaveShortcut.ts';
import useTabSave from './useTabSave.ts';

export default function ConfigEditorPage() {
  const { t } = useTranslations();
  const { t: tExt } = useExtTranslations();
  const server = useServerStore((state) => state.server);
  const canSave = useServerCan('files.create');

  const { tabs, activeTab, opening, setActivePath, open, update, load, close } = useEditorTabs(server.uuid);
  const { saving, conflict, dismissConflict, save, reload } = useTabSave(server.uuid, load);
  const [pendingClose, setPendingClose] = useState<string | null>(null);

  const conflictTab = tabs.find((tab) => tab.path === conflict) ?? null;
  const hasUnsavedChanges = tabs.some(isTabDirty);

  const blocker = useBlocker(hasUnsavedChanges, true);
  useBeforeUnload((event) => {
    if (hasUnsavedChanges) {
      event.preventDefault();
      event.returnValue = '';
    }
  });

  useSaveShortcut(() => {
    if (canSave && activeTab && saving === null && isTabSavable(activeTab)) save(activeTab);
  });

  const requestClose = (path: string) => {
    const tab = tabs.find((current) => current.path === path);
    if (tab && isTabDirty(tab)) {
      setPendingClose(path);
    } else {
      close(path);
    }
  };

  return (
    <ServerContentContainer title={tExt('common.configEditor', {})} subtitle={tExt('common.subtitle', {})}>
      <ConfirmationModal
        title={t('pages.server.files.modal.unsavedChanges.title', {})}
        opened={blocker.state === 'blocked'}
        onClose={blocker.reset}
        onConfirmed={blocker.proceed}
        confirm={t('common.button.leavePage', {})}
      >
        {t('pages.server.files.modal.unsavedChanges.content', {}).md()}
      </ConfirmationModal>

      <ConfirmationModal
        title={tExt('tabs.closeTitle', {})}
        opened={pendingClose !== null}
        onClose={() => setPendingClose(null)}
        onConfirmed={() => {
          if (pendingClose) close(pendingClose);
          setPendingClose(null);
        }}
        confirm={t('common.button.discard', {})}
      >
        {tExt('tabs.closeContent', { name: basename(pendingClose ?? '') })}
      </ConfirmationModal>

      <ConflictModal
        name={conflictTab && basename(conflictTab.path)}
        loading={saving !== null}
        onClose={dismissConflict}
        onReload={() => conflictTab && reload(conflictTab)}
        onOverwrite={() => conflictTab && save(conflictTab, true)}
      />

      <div className='grid grid-cols-1 gap-4 lg:h-[calc(100dvh-12rem)] lg:min-h-[32rem] lg:grid-cols-[17rem_minmax(0,1fr)]'>
        <SidePanel serverUuid={server.uuid} activePath={activeTab?.path ?? null} onOpen={open} />

        <Card p={0} className='flex min-h-[32rem] min-w-0 flex-col overflow-hidden lg:min-h-0'>
          {tabs.length > 0 && (
            <EditorTabBar
              tabs={tabs}
              activePath={activeTab?.path ?? null}
              opening={opening}
              onSelect={setActivePath}
              onClose={requestClose}
            />
          )}
          {opening && tabs.length === 0 && <Spinner.Centered className='py-4' />}
          {activeTab ? (
            <TabEditor
              key={activeTab.path}
              tab={activeTab}
              canSave={canSave}
              saving={saving === activeTab.path}
              onUpdate={(change) => update(activeTab.path, change)}
              onSave={() => save(activeTab)}
            />
          ) : (
            !opening && <WelcomeState />
          )}
        </Card>
      </div>
    </ServerContentContainer>
  );
}
