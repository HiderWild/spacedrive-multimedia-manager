import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';

/**
 * Translates backend error codes to localized strings.
 * Falls back to the provided English message if no translation exists.
 */
export function useErrorTranslation() {
	const { t } = useTranslation('errors');

	return useCallback(
		(code: string, fallback?: string): string => {
			const translated = t(code);
			// i18next returns the key itself when no translation is found
			return translated !== code ? translated : fallback || code;
		},
		[t]
	);
}
