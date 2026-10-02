import Alert from '@/elements/feedback/Alert.tsx';
import type { ParseError } from '../lib/tabs.ts';
import { useExtTranslations } from '../translations.ts';

export default function ParseErrorAlert({ error }: { error: ParseError }) {
  const { t: tExt } = useExtTranslations();
  const { message, line, column } = error;

  return (
    <Alert color='red' title={tExt('parseError.title', {})}>
      {line === null
        ? message
        : column === null
          ? tExt('parseError.line', { line, message })
          : tExt('parseError.location', { line, column, message })}
    </Alert>
  );
}
