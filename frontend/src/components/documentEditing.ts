import { createContext, useContext } from 'react';
import type { Format, Node } from '../lib/node.ts';

export interface DocumentEditing {
  format: Format;
  readOnly: boolean;
  edit: (update: (document: Node) => Node) => void;
}

export const DocumentEditingContext = createContext<DocumentEditing | null>(null);

export function useDocumentEditing(): DocumentEditing {
  const context = useContext(DocumentEditingContext);
  if (!context) throw new Error('useDocumentEditing must be used inside the visual editor');

  return context;
}
