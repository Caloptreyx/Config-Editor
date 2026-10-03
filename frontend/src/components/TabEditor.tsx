import { basename, dirname } from 'pathe';
import { useState } from 'react';
import { createSearchParams } from 'react-router';
import { httpErrorToHuman } from '@/api/axios.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';
import { saveDocument } from '../api.ts';
import { isDocumentValid, type Node } from '../lib/node.ts';
import { type EditorTab, isTabDirty, isTabSavable, tabFromFile } from '../lib/tabs.ts';
import { useExtTranslations } from '../translations.ts';
import ParseErrorAlert from './ParseErrorAlert.tsx';
import RawEditor from './RawEditor.tsx';
import ReviewChangesModal, { type ReviewDiff } from './ReviewChangesModal.tsx';
import TabToolbar from './TabToolbar.tsx';
import VisualEditor from './VisualEditor.tsx';

export default function TabEditor({
  tab,
  canSave,
  saving,
  onUpdate,
  onSave,
}: {
  tab: EditorTab;
  /** Without `files.create` the tab is read-only and the save actions are hidden. */
  canSave: boolean;
  saving: boolean;
  onUpdate: (update: (tab: EditorTab) => EditorTab) => void;
  onSave: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();
  const server = useServerStore((state) => state.server);

  const [review, setReview] = useState<ReviewDiff | null>(null);
  const [reviewing, setReviewing] = useState(false);

  const name = basename(tab.path);
  const dirty = isTabDirty(tab);
  const document = tab.raw ? null : tab.document;
  const invalid = !!document && !isDocumentValid(document);

  const doReview = () => {
    const base = { name, format: tab.file.format, original: tab.file.content };
    if (!document) {
      setReview({ ...base, modified: tab.content });
      return;
    }

    setReviewing(true);
    saveDocument(server.uuid, tab.path, document, { expectedHash: tab.file.hash }, true)
      .then((result) => setReview({ ...base, modified: result.content }))
      .catch((error) => addToast(httpErrorToHuman(error), 'error'))
      .finally(() => setReviewing(false));
  };

  const editDocument = (update: (document: Node) => Node) =>
    onUpdate((current) => (current.document ? { ...current, document: update(current.document) } : current));

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <ReviewChangesModal diff={review} onClose={() => setReview(null)} />

      <TabToolbar
        path={tab.path}
        format={tab.file.format}
        raw={tab.raw}
        rawLockedReason={
          !tab.file.document ? tExt('toolbar.rawModeUnavailable', {}) : dirty ? tExt('toolbar.rawModeLocked', {}) : null
        }
        dirty={dirty}
        savable={isTabSavable(tab)}
        invalid={invalid}
        canSave={canSave}
        saving={saving}
        reviewing={reviewing}
        fileEditorUrl={`/server/${server.uuidShort}/files/edit?${createSearchParams({
          directory: dirname(tab.path),
          file: basename(tab.path),
        })}`}
        onSave={onSave}
        onRevert={() => onUpdate((current) => tabFromFile(current.file, current.raw))}
        onReview={doReview}
        onToggleRaw={() => onUpdate((current) => ({ ...current, raw: !current.raw }))}
      />

      {tab.file.error && (
        <div className='shrink-0 px-4 pt-3'>
          <ParseErrorAlert error={tab.file.error} />
        </div>
      )}

      {document ? (
        <VisualEditor
          key={tab.path}
          document={document}
          format={tab.file.format}
          readOnly={!canSave}
          onEdit={editDocument}
        />
      ) : (
        <RawEditor
          path={tab.path}
          format={tab.file.format}
          content={tab.content}
          readOnly={!canSave}
          onChange={(content) => onUpdate((current) => ({ ...current, content }))}
        />
      )}
    </div>
  );
}
