import { initialize as initializeServices, getService, IWorkingCopyService, LogLevel } from '@codingame/monaco-vscode-api';
import { SyncDescriptor } from '@codingame/monaco-vscode-api/vscode/vs/platform/instantiation/common/descriptors';
import { URI } from '@codingame/monaco-vscode-api/vscode/vs/base/common/uri';
import { Schemas } from '@codingame/monaco-vscode-api/vscode/vs/base/common/network';
import { ISearchService } from '@codingame/monaco-vscode-api/vscode/vs/workbench/services/search/common/search.service';
import { SearchProviderType } from '@codingame/monaco-vscode-api/vscode/vs/workbench/services/search/common/search';
import { SearchHistoryService } from '@codingame/monaco-vscode-api/vscode/vs/workbench/contrib/search/common/searchHistoryService';
import { ISearchHistoryService } from '@codingame/monaco-vscode-api/vscode/vs/workbench/contrib/search/common/searchHistoryService.service';
import { ReplaceService } from '@codingame/monaco-vscode-api/vscode/vs/workbench/contrib/search/browser/replaceService';
import { IReplaceService } from '@codingame/monaco-vscode-api/vscode/vs/workbench/contrib/search/browser/replace.service';
import { ISearchViewModelWorkbenchService } from '@codingame/monaco-vscode-api/vscode/vs/workbench/contrib/search/browser/searchTreeModel/searchViewModelWorkbenchService.service';
import { SearchService } from '@codingame/monaco-vscode-search-service-override/vscode/vs/workbench/services/search/common/searchService';
import { SearchViewModelWorkbenchService } from '@codingame/monaco-vscode-search-service-override/vscode/vs/workbench/contrib/search/browser/searchTreeModel/searchModel';
import '@codingame/monaco-vscode-search-service-override/vscode/vs/workbench/contrib/search/browser/search.contribution';
import getAccessibilityServiceOverride from '@codingame/monaco-vscode-accessibility-service-override';
import getBulkEditServiceOverride from '@codingame/monaco-vscode-bulk-edit-service-override';
import getConfigurationServiceOverride, {
  initUserConfiguration,
} from '@codingame/monaco-vscode-configuration-service-override';
import getDialogsServiceOverride from '@codingame/monaco-vscode-dialogs-service-override';
import getExplorerServiceOverride from '@codingame/monaco-vscode-explorer-service-override';
import { registerFileSystemOverlay } from '@codingame/monaco-vscode-files-service-override';
import getKeybindingsServiceOverride from '@codingame/monaco-vscode-keybindings-service-override';
import getLanguagesServiceOverride from '@codingame/monaco-vscode-languages-service-override';
import getLifecycleServiceOverride from '@codingame/monaco-vscode-lifecycle-service-override';
import getLogServiceOverride from '@codingame/monaco-vscode-log-service-override';
import getMarkersServiceOverride from '@codingame/monaco-vscode-markers-service-override';
import getModelServiceOverride from '@codingame/monaco-vscode-model-service-override';
import getNotificationsServiceOverride from '@codingame/monaco-vscode-notifications-service-override';
import getOutlineServiceOverride from '@codingame/monaco-vscode-outline-service-override';
import getOutputServiceOverride from '@codingame/monaco-vscode-output-service-override';
import getPreferencesServiceOverride from '@codingame/monaco-vscode-preferences-service-override';
import getQuickAccessServiceOverride from '@codingame/monaco-vscode-quickaccess-service-override';
import getSnippetsServiceOverride from '@codingame/monaco-vscode-snippets-service-override';
import getStorageServiceOverride from '@codingame/monaco-vscode-storage-service-override';
import getTextmateServiceOverride from '@codingame/monaco-vscode-textmate-service-override';
import getThemeServiceOverride from '@codingame/monaco-vscode-theme-service-override';
import getUserDataProfileServiceOverride from '@codingame/monaco-vscode-user-data-profile-service-override';
import getStatusBarServiceOverride from '@codingame/monaco-vscode-view-status-bar-service-override';
import getTitleBarServiceOverride from '@codingame/monaco-vscode-view-title-bar-service-override';
import getWorkbenchServiceOverride from '@codingame/monaco-vscode-workbench-service-override';
import getWorkingCopyServiceOverride from '@codingame/monaco-vscode-working-copy-service-override';
import getWorkspaceTrustServiceOverride from '@codingame/monaco-vscode-workspace-trust-service-override';
import editorWorkerUrl from '@codingame/monaco-vscode-api/workers/editor.worker?worker&url';
import textMateWorkerUrl from '@codingame/monaco-vscode-textmate-service-override/worker?worker&url';
import outputLinkWorkerUrl from '@codingame/monaco-vscode-output-service-override/worker?worker&url';
import './extensions.ts';
import { PanelFiles } from './api.ts';
import { ServerFileSystemProvider } from './fileSystem.ts';
import { ServerSearchProvider } from './search.ts';

// Messages exchanged with the Config Editor page that hosts this frame.
export type HostMessage = { type: 'config-editor-vscode:dirty'; dirty: boolean };

const params = new URLSearchParams(location.search);
const serverUuid = params.get('server');
const serverName = params.get('name') || 'server';
const dark = params.get('theme') !== 'light';
const permissions = {
  write: params.get('write') === '1',
  delete: params.get('delete') === '1',
  rename: params.get('rename') === '1',
};
const maxContentSearchSize = Number(params.get('searchSize')) || 5 * 1024 * 1024;

function post(message: HostMessage) {
  if (window.parent !== window) window.parent.postMessage(message, location.origin);
}

async function start(container: HTMLElement) {
  if (!serverUuid) throw new Error('missing server parameter');
  const files = new PanelFiles(serverUuid);

  const workerUrls: Record<string, string> = {
    editorWorkerService: editorWorkerUrl,
    TextMateWorker: textMateWorkerUrl,
    OutputLinkDetectionWorker: outputLinkWorkerUrl,
  };
  window.MonacoEnvironment = {
    getWorkerUrl(_moduleId: string, label: string) {
      const url = workerUrls[label];
      if (!url) throw new Error(`no worker bundled for ${label}`);
      return url;
    },
  };

  registerFileSystemOverlay(1, new ServerFileSystemProvider(files, permissions));

  await initUserConfiguration(
    JSON.stringify({
      'workbench.colorTheme': dark ? 'Dark Modern' : 'Light Modern',
      'workbench.iconTheme': 'vs-seti',
      'workbench.startupEditor': 'none',
      'workbench.tips.enabled': false,
      'workbench.layoutControl.enabled': false,
      'window.commandCenter': true,
      'files.autoSave': 'off',
      'search.followSymlinks': false,
      'editor.minimap.enabled': true,
      'security.workspace.trust.enabled': false,
      'extensions.ignoreRecommendations': true,
    }),
  );

  const searchProvider = new ServerSearchProvider(files, () => maxContentSearchSize);

  // the stock search override indexes the whole workspace on startup, which on a game server
  // means listing every folder over HTTP, so the base service is registered with a provider
  // that asks Wings instead
  class ServerSearchService extends SearchService {
    constructor(...args: ConstructorParameters<typeof SearchService>) {
      super(...args);
      this.registerSearchResultProvider(Schemas.file, SearchProviderType.file, searchProvider);
      this.registerSearchResultProvider(Schemas.file, SearchProviderType.text, searchProvider);
    }
  }

  await initializeServices(
    {
      ...getLogServiceOverride(),
      ...getModelServiceOverride(),
      ...getNotificationsServiceOverride(),
      ...getDialogsServiceOverride(),
      ...getConfigurationServiceOverride(),
      ...getKeybindingsServiceOverride(),
      ...getTextmateServiceOverride(),
      ...getThemeServiceOverride(),
      ...getLanguagesServiceOverride(),
      ...getPreferencesServiceOverride(),
      ...getOutlineServiceOverride(),
      ...getOutputServiceOverride(),
      ...getMarkersServiceOverride(),
      ...getAccessibilityServiceOverride(),
      ...getSnippetsServiceOverride(),
      ...getBulkEditServiceOverride(),
      ...getStorageServiceOverride(),
      ...getLifecycleServiceOverride(),
      ...getUserDataProfileServiceOverride(),
      ...getWorkingCopyServiceOverride(),
      ...getWorkspaceTrustServiceOverride(),
      ...getExplorerServiceOverride(),
      ...getStatusBarServiceOverride(),
      ...getTitleBarServiceOverride(),
      ...getWorkbenchServiceOverride(),
      ...getQuickAccessServiceOverride({
        isKeybindingConfigurationVisible: () => true,
        shouldUseGlobalPicker: () => true,
      }),
      [ISearchService.toString()]: new SyncDescriptor(ServerSearchService, [], true),
      [ISearchHistoryService.toString()]: new SyncDescriptor(SearchHistoryService, [], true),
      [IReplaceService.toString()]: new SyncDescriptor(ReplaceService, [], true),
      [ISearchViewModelWorkbenchService.toString()]: new SyncDescriptor(SearchViewModelWorkbenchService, [], true),
    },
    container,
    {
      workspaceProvider: {
        trusted: true,
        workspace: { folderUri: URI.file('/') },
        async open() {
          return false;
        },
      },
      windowIndicator: { label: serverName, tooltip: serverName, command: '' },
      productConfiguration: { nameShort: 'VS Code', nameLong: `VS Code: ${serverName}` },
      developmentOptions: { logLevel: LogLevel.Warning },
      configurationDefaults: { 'window.title': '${dirty}${activeEditorShort}${separator}${rootName}' },
    },
    { userHome: URI.file('/') },
  );

  const workingCopies = await getService(IWorkingCopyService);
  post({ type: 'config-editor-vscode:dirty', dirty: workingCopies.hasDirty });
  workingCopies.onDidChangeDirty(() => post({ type: 'config-editor-vscode:dirty', dirty: workingCopies.hasDirty }));
}

const container = document.getElementById('workbench');
if (container) {
  start(container).catch((error: unknown) => {
    container.textContent = `VS Code failed to start: ${error instanceof Error ? error.message : String(error)}`;
    container.style.cssText += ';color:#ccc;font:14px sans-serif;padding:24px';
    console.error(error);
  });
}
