import { faSliders } from '@fortawesome/free-solid-svg-icons';
import { Extension, ExtensionContext } from 'shared';
import ConfigEditorPage from './pages/ConfigEditorPage.tsx';
import { getExtTranslations } from './translations.ts';

class CaloptreyxConfigEditorExtension extends Extension {
  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.enterRoutes((routes) =>
      routes.addServerRoute({
        name: () => getExtTranslations().t('common.configEditor', {}),
        icon: faSliders,
        path: '/config-editor',
        element: ConfigEditorPage,
        permission: 'files.read-content',
      }),
    );
  }
}

export default new CaloptreyxConfigEditorExtension();
