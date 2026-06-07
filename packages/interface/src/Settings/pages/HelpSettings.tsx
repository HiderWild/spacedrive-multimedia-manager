import {useTranslation} from 'react-i18next';
import {organizeHelpSections} from './helpSettingsContent';

export function HelpSettings() {
	const {t} = useTranslation('settings');

	return (
		<div className="space-y-6">
			<div>
				<h2 className="text-ink mb-2 text-lg font-semibold">
					{t('help.title')}
				</h2>
				<p className="text-ink-dull text-sm">{t('help.description')}</p>
			</div>

			<div className="border-app-line bg-app-box rounded-lg border p-4">
				<h3 className="text-ink mb-1 text-sm font-medium">
					{t('help.organizeTitle')}
				</h3>
				<p className="text-ink-dull text-xs leading-6">
					{t('help.organizeDescription')}
				</p>
			</div>

			<div className="grid gap-4 xl:grid-cols-3">
				{organizeHelpSections.map((section) => (
					<section
						key={section.id}
						className="border-app-line bg-app-box rounded-lg border p-4"
					>
						<h3 className="text-ink mb-4 text-sm font-medium">
							{t(section.titleKey)}
						</h3>
						<div className="space-y-3">
							{section.items.map((item) => (
								<div
									key={`${section.id}-${item.keys}`}
									className="flex items-start justify-between gap-4"
								>
									<kbd className="border-app-line bg-app text-ink rounded-md border px-2 py-1 text-xs font-medium">
										{item.keys}
									</kbd>
									<p className="text-ink-dull flex-1 text-right text-xs leading-5">
										{t(item.descriptionKey)}
									</p>
								</div>
							))}
						</div>
					</section>
				))}
			</div>
		</div>
	);
}
