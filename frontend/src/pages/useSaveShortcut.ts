import { useEffect, useRef } from 'react';

// Ctrl/Cmd+S anywhere on the page, including inside Monaco (the capture phase runs before the editor sees it)
export default function useSaveShortcut(onSave: () => void) {
  const onSaveRef = useRef(onSave);

  useEffect(() => {
    onSaveRef.current = onSave;
  });

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== 's')
        return;

      event.preventDefault();
      onSaveRef.current();
    };

    window.addEventListener('keydown', handler, true);
    return () => window.removeEventListener('keydown', handler, true);
  }, []);
}
