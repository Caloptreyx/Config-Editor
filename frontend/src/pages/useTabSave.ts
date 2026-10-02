import { basename } from 'pathe';
import { useState } from 'react';
import { getHttpStatus, httpErrorToHuman } from '@/api/axios.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { getConfigFile, saveDocument, saveRawContent } from '../api.ts';
import type { ConfigFile, EditorTab } from '../lib/tabs.ts';
import { useExtTranslations } from '../translations.ts';

// saves tabs through the visual or raw endpoint; a 409 parks the tab path in `conflict` for the user to resolve
export default function useTabSave(serverUuid: string, onLoaded: (file: ConfigFile, raw: boolean) => void) {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();
  const [saving, setSaving] = useState<string | null>(null);
  const [conflict, setConflict] = useState<string | null>(null);

  const save = async (tab: EditorTab, force = false) => {
    const name = basename(tab.path);
    const options = { expectedHash: tab.file.hash, force };

    setSaving(tab.path);
    try {
      const saved =
        tab.raw || !tab.document
          ? await saveRawContent(serverUuid, tab.path, tab.content, options)
          : await saveDocument(serverUuid, tab.path, tab.document, options);

      onLoaded(saved, tab.raw);
      setConflict(null);
      addToast(saved.changed ? tExt('toast.saved', { name }) : tExt('toast.unchanged', { name }), 'success');
    } catch (error) {
      if (!force && getHttpStatus(error) === 409) {
        setConflict(tab.path);
      } else {
        addToast(httpErrorToHuman(error), 'error');
      }
    } finally {
      setSaving(null);
    }
  };

  const reload = async (tab: EditorTab) => {
    setSaving(tab.path);
    try {
      onLoaded(await getConfigFile(serverUuid, tab.path), tab.raw);
      setConflict(null);
      addToast(tExt('toast.reloaded', { name: basename(tab.path) }), 'success');
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    } finally {
      setSaving(null);
    }
  };

  return { saving, conflict, dismissConflict: () => setConflict(null), save, reload };
}
