import {useCallback, useEffect, useMemo, useRef, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {ArrowSquareOut} from '@phosphor-icons/react';
import type {File} from '@sd/ts-client';
import {usePlatform} from '../../../contexts/PlatformContext';
import {useExplorer} from '../context';
import {useExplorerFiles} from '../hooks/useExplorerFiles';
import {useSelection} from '../SelectionContext';
import {GridView} from '../views/GridView';
import {canUseOrganizeView} from './organizeAvailability';
import {OrganizeCenterPane} from './OrganizeCenterPane';
import {openOrganizeDeleteDialog} from './OrganizeDeleteDialog';
import {OrganizeLayout} from './OrganizeLayout';
import {OrganizeLeftPane} from './OrganizeLeftPane';
import {OrganizePreviewContent} from './OrganizePreviewContent';
import {OrganizeDebugPanel} from './OrganizeDebugPanel';
import {deriveOrganizeInspectorPreview, type DirectoryPreviewAvailability} from './organizePreview';
import {collectDiscardDeleteTargets} from './organizeState';
import type {OrganizeCenterLayout, OrganizeLeftTab} from './organizeTypes';
import {useOrganizeState} from './useOrganizeState';

export function OrganizeView() {
	const {t} = useTranslation('explorer');
	const platform = usePlatform();
	const explorer = useExplorer();
	const {files, isLoading} = useExplorerFiles();
	const {selectedFiles, selectFile, restoreSelectionFromFiles} =
		useSelection();
	const organize = useOrganizeState({
		currentPath: explorer.currentPath,
		files
	});
	const [leftTab, setLeftTab] = useState<OrganizeLeftTab>('keep');
	const [layout, setLayout] = useState<OrganizeCenterLayout>('grid');
	const initialInspectorVisible = useRef(explorer.inspectorVisible);
	const setInspectorVisible = explorer.setInspectorVisible;

	const deleteTargets = useMemo(
		() =>
			organize.state
				? collectDiscardDeleteTargets(files, organize.state)
				: [],
		[files, organize.state]
	);

	const handleDeleteClick = useCallback(() => {
		if (deleteTargets.length === 0) return;
		openOrganizeDeleteDialog({
			files: deleteTargets,
			onDeleted: organize.removeDeleted
		});
	}, [deleteTargets, organize.removeDeleted]);

	const handleNavigateToDirectory = useCallback(
		async (file: File) => {
			if (file.kind !== 'Directory' || !file.sd_path) return;

			// Flush pending organize state before navigation
			await organize.flushPending();

			// Navigate to the directory
			explorer.navigateToPath(file.sd_path);
		},
		[organize, explorer]
	);

	useEffect(() => {
		explorer.setCurrentFiles(files);
		restoreSelectionFromFiles(files);
	}, [explorer, files, restoreSelectionFromFiles]);

	useEffect(() => {
		if (!initialInspectorVisible.current) {
			setInspectorVisible(true);
		}
	}, [setInspectorVisible]);

	if (
		!canUseOrganizeView({
			platform,
			mode: explorer.mode,
			currentPath: explorer.currentPath
		})
	) {
		return <GridView />;
	}

	if (isLoading || organize.isLoading || !organize.state) {
		return (
			<div className="text-ink-dull flex h-full items-center justify-center text-sm">
				{t('organize.title')}…
			</div>
		);
	}

	const selectedFile = selectedFiles[0] ?? null;

	const [showDebug, setShowDebug] = useState(false);

	// Derive directory preview availability if selected file is a directory
	const directoryAvailability = useMemo(() => {
		if (!selectedFile || selectedFile.kind !== 'Directory') return null;
		// For now, return basic availability - in production this would query directory contents
		return {
			renderedTabs: ['list'],
			enabledTabs: ['list'],
			defaultTab: 'list',
			firstVideo: null,
			firstImage: null
		} as DirectoryPreviewAvailability;
	}, [selectedFile]);

	const previewState = useMemo(() => {
		return deriveOrganizeInspectorPreview({
			selectedFile,
			directoryAvailability
		});
	}, [selectedFile, directoryAvailability]);

	return (
		<OrganizeLayout
			left={
				<OrganizeLeftPane
					leftTab={leftTab}
					onLeftTabChange={setLeftTab}
					keepFiles={organize.keepFiles}
					discardFiles={organize.discardFiles}
					onRevealItem={(file) =>
						selectFile(file, files, false, false)
					}
					onDeleteClick={handleDeleteClick}
				/>
			}
			center={
				<OrganizeCenterPane
					selectedFileId={selectedFile?.id ?? null}
					layout={layout}
					onLayoutChange={setLayout}
					presentation={organize.presentation}
					onSelectFile={(file) =>
						selectFile(file, files, false, false)
					}
					onMarkKeep={organize.markKeep}
					onMarkDiscard={organize.markDiscard}
					onClearDecision={organize.clearDecision}
					onNavigateToDirectory={handleNavigateToDirectory}
				/>
			}
			right={
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

					{/* Preview or debug panel */}
					<div className="min-h-0 flex-1">
						{showDebug && selectedFile ? (
							<OrganizeDebugPanel
								title="Preview State"
								payload={{selectedFile: selectedFile.name, previewState}}
							/>
						) : selectedFile && previewState.defaultTabId ? (
							<OrganizePreviewContent
								selectedFile={selectedFile}
								activeTab={previewState.defaultTabId}
								context={{sortBy: 'name', foldersFirst: false}}
							/>
						) : (
							<div className="flex h-full items-center justify-center p-4 text-sm text-ink-dull">
								{t('organize.selectFileToPreview')}
							</div>
						)}
					</div>
				</div>
			}
		/>
	);
}
