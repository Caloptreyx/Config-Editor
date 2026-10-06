import { faSliders } from '@fortawesome/free-solid-svg-icons';
import { lazy } from 'react';
import { Extension, ExtensionContext } from 'shared';
import ConfigEditorPage from './pages/ConfigEditorPage.tsx';
import { getExtTranslations } from './translations.ts';

const VsCodePage = lazy(() => import('./pages/VsCodePage.tsx'));

class CaloptreyxConfigEditorExtension extends Extension {
  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.enterRoutes((routes) =>
      routes
        .addServerRoute({
          name: () => getExtTranslations().t('common.configEditor', {}),
          icon: faSliders,
          path: '/config-editor',
          element: ConfigEditorPage,
          permission: 'files.read-content',
        })
        .addServerRoute({
          name: undefined,
          path: '/config-editor/vscode',
          element: VsCodePage,
          permission: 'files.read-content',
        }),
    );
  }
}

export default new CaloptreyxConfigEditorExtension();
