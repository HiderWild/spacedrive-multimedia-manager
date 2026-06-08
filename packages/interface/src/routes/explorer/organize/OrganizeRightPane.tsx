import {useState} from 'react';
import {useTranslation} from 'react-i18next';
import type {File} from '@sd/ts-client';
import {OrganizeDebugPanel} from './OrganizeDebugPanel';
import {OrganizePreviewContent} from './OrganizePreviewContent';
import {deriveOrganizeInspectorPreview, type DirectoryPreviewAvailability} from './organizePreview';

export function OrganizeRightPane(props: {
	selectedFile: File | null;
	directoryAvailability: DirectoryPreviewAvailability | null;
}) {
	const {t} = useTranslation('explorer');
	const [showDebug, setShowDebug] = useState(false);

	const previewState = deriveOrganizeInspectorPreview({
		selectedFile: props.selectedFile,
		directoryAvailability: props.directoryAvailability
	});

	return (
		<div className="flex h-full min-h-0 flex-col">
			{/* Debug toggle button */}
			<div className="flex justify-end border-b border-app-line p-2">
				<button
					type="button"
					onClick={() => setShowDebug(!showDebug)}
					className="rounded-md px-2 py-1 text-xs text-ink-dull hover:bg-app-hover hover:text-ink"
				>
					{showDebug ? 'Hide' : 'Show'} Debug
				</button>
			</div>

			{/* Debug info section - shown when enabled */}
			{showDebug && props.selectedFile && (
				<div className="border-b border-app-line">
					<OrganizeDebugPanel
						title="Preview State"
						payload={{
							selectedFile: props.selectedFile.name,
							previewState
						}}
					/>
				</div>
			)}

			{/* Preview content or placeholder */}
			<div className="min-h-0 flex-1">
				{props.selectedFile && previewState.defaultTabId ? (
					<OrganizePreviewContent
						selectedFile={props.selectedFile}
						activeTab={previewState.defaultTabId}
						context={{sortBy: 'name', foldersFirst: false}}
					/>
				) : (
					<div className="flex h-full items-center justify-center px-4 text-center">
						<p className="text-sidebar-inkDull text-xs">
							{t('inspector.selectAnItemToViewDetails')}
						</p>
					</div>
				)}
			</div>
		</div>
	);
}
