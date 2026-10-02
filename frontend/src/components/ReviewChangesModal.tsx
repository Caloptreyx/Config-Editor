import { MonacoDiffEditor } from '@/elements/editors/MonacoEditor.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import { Modal } from '@/elements/modals/Modal.tsx';
import { registerTomlLanguage } from '@/lib/editor/monaco.ts';
import { monacoLanguage } from '../lib/formats.ts';
import type { Format } from '../lib/node.ts';
import { useExtTranslations } from '../translations.ts';

export interface ReviewDiff {
  name: string;
  format: Format;
  original: string;
  modified: string;
}

// what saving would write: the file on disk next to the content the backend produced for the edits
export default function ReviewChangesModal({ diff, onClose }: { diff: ReviewDiff | null; onClose: () => void }) {
  const { t: tExt } = useExtTranslations();

  return (
    <Modal
      title={diff && tExt('review.title', { name: diff.name })}
      opened={diff !== null}
      onClose={onClose}
      size='90%'
    >
      {diff &&
        (diff.original === diff.modified ? (
          <Alert color='blue'>{tExt('review.noChanges', {})}</Alert>
        ) : (
          <div className='flex h-[70vh]'>
            <MonacoDiffEditor
              height='100%'
              width='100%'
              language={monacoLanguage(diff.format)}
              original={diff.original}
              modified={diff.modified}
              beforeMount={registerTomlLanguage}
              options={{
                readOnly: true,
                originalEditable: false,
                minimap: { enabled: false },
                codeLens: false,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        ))}
    </Modal>
  );
}
