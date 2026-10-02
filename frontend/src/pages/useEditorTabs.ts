import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { getConfigFile } from '../api.ts';
import { type ConfigFile, type EditorTab, nextActivePath, tabFromFile, upsertTab } from '../lib/tabs.ts';

export default function useEditorTabs(serverUuid: string) {
  const { addToast } = useToast();
  const [tabs, setTabs] = useState<EditorTab[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  const open = (path: string) => {
    if (tabs.some((tab) => tab.path === path)) {
      setActivePath(path);
      return;
    }

    setOpening(true);
    getConfigFile(serverUuid, path)
      .then((file) => {
        // a second click may have opened it meanwhile; never replace a tab that may hold edits
        setTabs((current) =>
          current.some((tab) => tab.path === file.path) ? current : [...current, tabFromFile(file)],
        );
        setActivePath(file.path);
      })
      .catch((error) => addToast(httpErrorToHuman(error), 'error'))
      .finally(() => setOpening(false));
  };

  const update = (path: string, change: (tab: EditorTab) => EditorTab) =>
    setTabs((current) => current.map((tab) => (tab.path === path ? change(tab) : tab)));

  /** Replaces the tab of `file.path` with the server state, dropping its edits. */
  const load = (file: ConfigFile, raw: boolean) => setTabs((current) => upsertTab(current, tabFromFile(file, raw)));

  const close = (path: string) => {
    setActivePath(nextActivePath(tabs, path, activePath));
    setTabs((current) => current.filter((tab) => tab.path !== path));
  };

  return {
    tabs,
    activeTab: tabs.find((tab) => tab.path === activePath) ?? null,
    opening,
    setActivePath,
    open,
    update,
    load,
    close,
  };
}
