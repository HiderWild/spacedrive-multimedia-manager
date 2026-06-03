import { useTranslation } from 'react-i18next';
import { Select, SelectTrigger, SelectContent, SelectItem } from '@radix-ui/react-select';

const LANGUAGES = [
	{ code: 'zh', label: '中文' },
	{ code: 'en', label: 'English' },
] as const;

export function LanguageSelector() {
	const { i18n, t } = useTranslation('settings');

	const handleChange = (lang: string) => {
		i18n.changeLanguage(lang);
	};

	const currentLabel =
		LANGUAGES.find((l) => l.code === i18n.language)?.label ??
		LANGUAGES.find((l) => l.code === 'zh')?.label ??
		'中文';

	return (
		<div className="p-4 bg-app-box rounded-lg border border-app-line">
			<h3 className="text-sm font-medium text-ink mb-1">{t('general.language')}</h3>
			<p className="text-xs text-ink-dull mb-3">{t('general.languageDescription')}</p>
			<Select value={i18n.language} onValueChange={handleChange}>
				<SelectTrigger className="w-full px-3 py-2 bg-app border border-app-line rounded-md text-ink text-sm focus:outline-none focus:ring-2 focus:ring-accent">
					{currentLabel}
				</SelectTrigger>
				<SelectContent className="bg-app-box border border-app-line rounded-lg shadow-lg">
					{LANGUAGES.map((lang) => (
						<SelectItem
							key={lang.code}
							value={lang.code}
							className="px-3 py-2 text-sm text-ink hover:bg-app-hover cursor-pointer"
						>
							{lang.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</div>
	);
}
