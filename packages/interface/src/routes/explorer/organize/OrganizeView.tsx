import {useCallback, useEffect, useMemo, useRef, useState} from 'react';
import {useTranslation} from 'react-i18next';
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
import {collectDiscardDeleteTargets} from './organizeState';
import type {OrganizeCenterLayout, OrganizeLeftTab} from './organizeTypes';
import {useOrganizeState} from './useOrganizeState';

export function OrganizeView() {
	const {t} = useTranslation('explorer');
	const platform = usePlatform();
	const explorer = useExplorer();
	const {files, isLoading, fetchNextPage, hasNextPage} = useExplorerFiles();
	const {selectedFiles, selectFile, restoreSelectionFromFiles} =
		useSelection();
	const organize = useOrganizeState({
		currentPath: explorer.currentPath,
		files
	});
	const [leftTab, setLeftTab] = useState<OrganizeLeftTab>('keep');
	const [layout, setLayout] = useState<OrganizeCenterLayout>('grid');
	const [multiSelectedIds, setMultiSelectedIds] = useState<Set<string>>(new Set());
	const initialInspectorVisible = useRef(explorer.inspectorVisible);
	const setInspectorVisible = explorer.setInspectorVisible;

	const selectedFile = selectedFiles[0] ?? null;

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

			// Clear multi-selection on navigation
			setMultiSelectedIds(new Set());

			// Flush pending organize state before navigation
			await organize.flushPending();

			// Navigate to the directory
			explorer.navigateToPath(file.sd_path);
		},
		[organize, explorer]
	);

	const handleSelectFile = useCallback(
		(file: File, isMulti: boolean = false) => {
			if (isMulti) {
				// Multi-select: don't update preview
				return;
			}
			// Single select: clear multi-selection and update preview
			setMultiSelectedIds(new Set());
			selectFile(file, files, false, false);
		},
		[files, selectFile]
	);

	const handleToggleMultiSelect = useCallback((fileId: string) => {
		setMultiSelectedIds(prev => {
			const next = new Set(prev);
			if (next.has(fileId)) {
				next.delete(fileId);
			} else {
				next.add(fileId);
			}
			return next;
		});
	}, []);

	const handleClearMultiSelect = useCallback(() => {
		setMultiSelectedIds(new Set());
	}, []);

	useEffect(() => {
		explorer.setCurrentFiles(files);
		restoreSelectionFromFiles(files);
	}, [explorer, files, restoreSelectionFromFiles]);

	useEffect(() => {
		if (!initialInspectorVisible.current) {
			setInspectorVisible(true);
		}
	}, [setInspectorVisible]);

	// Global Backspace key handler for navigation
	useEffect(() => {
		const handleGlobalKeyDown = (e: KeyboardEvent) => {
			if (e.key === 'Backspace' && !e.repeat) {
				// Check if we're not in an input field
				const target = e.target as HTMLElement;
				if (target.tagName !== 'INPUT' && target.tagName !== 'TEXTAREA' && !target.isContentEditable) {
					e.preventDefault();
					if (explorer.canGoBack) {
						explorer.goBack();
					}
				}
			}
		};

		window.addEventListener('keydown', handleGlobalKeyDown);
		return () => window.removeEventListener('keydown', handleGlobalKeyDown);
	}, [explorer]);

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
					multiSelectedIds={multiSelectedIds}
					layout={layout}
					onLayoutChange={setLayout}
					presentation={organize.presentation}
					onSelectFile={handleSelectFile}
					onToggleMultiSelect={handleToggleMultiSelect}
					onClearMultiSelect={handleClearMultiSelect}
					onMarkKeep={organize.markKeep}
					onMarkDiscard={organize.markDiscard}
					onClearDecision={organize.clearDecision}
					onNavigateToDirectory={handleNavigateToDirectory}
					onLoadMore={fetchNextPage}
					hasMore={hasNextPage}
				/>
			}
		/>
	);
}
