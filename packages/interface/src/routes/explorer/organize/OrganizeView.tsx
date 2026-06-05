import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePlatform } from "../../../contexts/PlatformContext";
import { useExplorer } from "../context";
import { useExplorerFiles } from "../hooks/useExplorerFiles";
import { useSelection } from "../SelectionContext";
import { GridView } from "../views/GridView";
import { canUseOrganizeView } from "./organizeAvailability";
import { collectDiscardDeleteTargets } from "./organizeState";
import { useOrganizeState } from "./useOrganizeState";
import { OrganizeLayout } from "./OrganizeLayout";
import { OrganizeLeftPane } from "./OrganizeLeftPane";
import { OrganizeCenterPane } from "./OrganizeCenterPane";
import { OrganizePreviewPane } from "./OrganizePreviewPane";
import { openOrganizeDeleteDialog } from "./OrganizeDeleteDialog";
import type { OrganizeLeftTab, OrganizeCenterLayout } from "./organizeTypes";

export function OrganizeView() {
	const { t } = useTranslation("explorer");
	const platform = usePlatform();
	const explorer = useExplorer();
	const { files, isLoading } = useExplorerFiles();
	const { selectedFiles, selectFile, restoreSelectionFromFiles } = useSelection();
	const organize = useOrganizeState({ currentPath: explorer.currentPath, files });
	const [leftTab, setLeftTab] = useState<OrganizeLeftTab>("keep");
	const [layout, setLayout] = useState<OrganizeCenterLayout>("grid");

	const deleteTargets = useMemo(
		() => (organize.state ? collectDiscardDeleteTargets(files, organize.state) : []),
		[files, organize.state],
	);

	const handleDeleteClick = useCallback(() => {
		if (deleteTargets.length === 0) return;
		openOrganizeDeleteDialog({
			files: deleteTargets,
			onDeleted: organize.removeDeleted,
		});
	}, [deleteTargets, organize.removeDeleted]);

	useEffect(() => {
		explorer.setCurrentFiles(files);
		restoreSelectionFromFiles(files);
	}, [explorer, files, restoreSelectionFromFiles]);

	if (!canUseOrganizeView({ platform, mode: explorer.mode, currentPath: explorer.currentPath })) {
		return <GridView />;
	}

	if (isLoading || organize.isLoading || !organize.state) {
		return (
			<div className="flex h-full items-center justify-center text-sm text-ink-dull">
				{t("organize.title")}…
			</div>
		);
	}

	const selectedFile = selectedFiles[0] ?? null;
	return (
		<OrganizeLayout
			left={
				<OrganizeLeftPane
					leftTab={leftTab}
					onLeftTabChange={setLeftTab}
					keepFiles={organize.keepFiles}
					discardFiles={organize.discardFiles}
					onRevealItem={(file) => selectFile(file, files, false, false)}
					onDeleteClick={handleDeleteClick}
				/>
			}
			center={
				<OrganizeCenterPane
					selectedFileId={selectedFile?.id ?? null}
					layout={layout}
					onLayoutChange={setLayout}
					presentation={organize.presentation}
					onSelectFile={(file) => selectFile(file, files, false, false)}
					onMarkKeep={organize.markKeep}
					onMarkDiscard={organize.markDiscard}
					onClearDecision={organize.clearDecision}
				/>
			}
			right={<OrganizePreviewPane selectedFile={selectedFile} />}
		/>
	);
}
