import { useTranslation } from 'react-i18next';

/**
 * OrganizeView is a placeholder for the full organize view implementation (Task 4).
 * For now it renders a minimal stub so ExplorerPaneBody can route to it.
 */
export function OrganizeView() {
	const { t } = useTranslation('explorer');

	return (
		<div className="flex h-full flex-col items-center justify-center p-8 text-center">
			<p className="text-ink-dull text-sm">
				{t('organize.placeholder')}
			</p>
		</div>
	);
}
