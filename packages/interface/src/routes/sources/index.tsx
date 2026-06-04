import { Plus, ArrowLeft } from "@phosphor-icons/react";
import { useNavigate } from "react-router-dom";
import { useLibraryQuery } from "../../contexts/SpacedriveContext";
import { useTabManager } from "../../components/TabManager/useTabManager";
import { SourceCard } from "../../components/Sources/SourceCard";
import { TopBarPortal, TopBarItem } from "../../TopBar";
import { CircleButton } from "@spacedrive/primitives";
import { SearchBar } from "@spacedrive/primitives";
import { useTranslation } from 'react-i18next';

export function SourcesHome() {
	const { t } = useTranslation('settings');
	const navigate = useNavigate();
	const { createTab } = useTabManager();
	const { data: sourcesRaw, isLoading, error } = useLibraryQuery({
		type: "sources.list",
		input: { data_type: null },
	});
	const sources = sourcesRaw as any[] | undefined;

	return (
		<>
			<TopBarPortal
				left={
					<>
						<TopBarItem id="back" label="Back" priority="high">
							<CircleButton
								icon={ArrowLeft}
								onClick={() => navigate(-1)}
							/>
						</TopBarItem>
						<TopBarItem id="title" label="Title" priority="high">
							<h1 className="text-ink text-xl font-bold">
								{t('sources.title')}
							</h1>
						</TopBarItem>
					</>
				}
				right={
					<>
						<TopBarItem id="search" label="Search" priority="high">
								<SearchBar
								placeholder={t('sources.searchPlaceholder')}
								value=""
								onChange={() => {}}
								onClear={() => {}}
								className="w-64"
							/>
						</TopBarItem>
						<TopBarItem id="add-source" label="Add Source" priority="high">
							<CircleButton
								icon={Plus}
								onClick={() => createTab("Adapters", "/sources/adapters")}
								title={t('sources.addSource')}
							/>
						</TopBarItem>
					</>
				}
			/>
			<div className="p-6">

			{isLoading && (
				<div className="flex items-center justify-center py-20">
					<div className="text-ink-faint text-sm">{t('sources.loading')}</div>
				</div>
			)}

			{error && (
				<div className="border-red-400/20 rounded-lg border p-4">
					<p className="text-sm text-red-400">
						{t('sources.loadError', { error: String(error) })}
					</p>
				</div>
			)}

			{sources && sources.length === 0 && (
				<div className="flex flex-col items-center justify-center py-20">
					<p className="text-ink-dull text-sm">{t('sources.noSources')}</p>
					<p className="text-ink-faint mt-1 text-xs">
						{t('sources.noSourcesDescription')}
					</p>
					<button
						onClick={() => createTab("Adapters", "/sources/adapters")}
						className="bg-accent hover:bg-accent-deep mt-4 rounded-lg px-3.5 py-1.5 text-sm font-medium text-white transition-colors"
					>
						{t('sources.addSource')}
					</button>
				</div>
			)}

			{sources && sources.length > 0 && (
				<div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{sources.map((source) => (
						<SourceCard key={source.id} source={source} />
					))}
				</div>
			)}
			</div>
		</>
	);
}
