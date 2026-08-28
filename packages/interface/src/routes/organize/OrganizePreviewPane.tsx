import {
	ArrowLeft,
	ArrowRight,
	FolderOpen,
	WarningCircle,
} from "@phosphor-icons/react";
import type {
	File,
	PreviewSequenceInput,
	PreviewSequenceOutput,
	SdPath,
} from "@sd/ts-client";
import { useLibraryQuery } from "@sd/ts-client/hooks";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ContentRenderer } from "../../components/QuickPreview/ContentRenderer";
import { DirectoryPreview } from "../../components/QuickPreview/DirectoryPreview";
import { File as FileComponent } from "../explorer/File";

export interface OrganizePreviewPaneProps {
	taskId: string;
	selectedFile: File | null;
	siblingFiles?: readonly File[];
	onSelectFile?: (file: File) => void;
}

export interface PreviewSequenceProps {
	directory: File;
	taskId: string;
	itemId: string;
}

export function findAdjacentPreviewFile(
	files: readonly File[],
	currentFileId: string,
	offset: -1 | 1,
): File | null {
	const index = files.findIndex((file) => file.id === currentFileId);
	if (index < 0) return null;
	return files[index + offset] ?? null;
}

export function previewSequenceInput(
	directory: SdPath,
	taskId: string,
	itemId: string,
): PreviewSequenceInput {
	return {
		directory,
		organize: { task_id: taskId, item_id: itemId },
		limit: 12,
	};
}

export function previewSequenceLabel(
	files: readonly File[],
	selectedFileId: string | null,
	candidateBudgetExhausted: boolean,
): string {
	const position = files.findIndex((file) => file.id === selectedFileId);
	const count =
		files.length > 0 && position >= 0
			? `${position + 1} / ${files.length}`
			: "No media";
	return candidateBudgetExhausted ? `${count} · sampled` : count;
}

export function PreviewSequence({
	directory,
	taskId,
	itemId,
}: PreviewSequenceProps) {
	const query = useLibraryQuery(
		{
			type: "files.preview_sequence",
			input: previewSequenceInput(directory.sd_path, taskId, itemId),
		},
		{
			staleTime: 30_000,
			refetchOnWindowFocus: false,
		},
	);
	const output: PreviewSequenceOutput | undefined = query.data;
	const files = output?.files ?? [];
	const [selectedFileId, setSelectedFileId] = useState<string | null>(null);

	useEffect(() => {
		if (!selectedFileId || !files.some((file) => file.id === selectedFileId)) {
			setSelectedFileId(files[0]?.id ?? null);
		}
	}, [files, selectedFileId]);

	const selectedFile =
		files.find((file) => file.id === selectedFileId) ?? files[0] ?? null;
	const moveSelection = useCallback(
		(offset: -1 | 1) => {
			if (!selectedFile) return;
			const next = findAdjacentPreviewFile(files, selectedFile.id, offset);
			if (next) setSelectedFileId(next.id);
		},
		[files, selectedFile],
	);

	if (query.isLoading) {
		return (
			<div className="flex h-full min-h-0 flex-col">
				<div className="flex min-h-0 flex-1 items-center justify-center bg-black/20">
					<div className="space-y-3 text-center text-sm text-ink-dull">
						<div className="mx-auto h-8 w-8 animate-pulse rounded-full bg-app-hover" />
						<div>Preparing a bounded preview…</div>
					</div>
				</div>
			</div>
		);
	}

	if (query.error) {
		return (
			<div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
				<WarningCircle size={28} className="text-ink-dull" />
				<div className="text-sm text-ink-dull">Preview unavailable for this folder.</div>
			</div>
		);
	}

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="relative min-h-0 flex-1 bg-black">
				{selectedFile ? (
					<ContentRenderer
						key={selectedFile.id}
						file={selectedFile}
						videoKeyboardShortcutsEnabled={false}
						videoWheelZoomEnabled={false}
					/>
				) : (
					<div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-ink-dull">
						<FolderOpen size={32} />
						<span>No image or video samples in this folder.</span>
					</div>
				)}
				{selectedFile && files.length > 1 && (
					<>
						<button
							type="button"
							className="absolute left-3 top-1/2 -translate-y-1/2 rounded-full bg-black/60 p-2 text-white transition hover:bg-black/80 disabled:opacity-30"
							onClick={() => moveSelection(-1)}
							disabled={!findAdjacentPreviewFile(files, selectedFile.id, -1)}
							aria-label="Previous preview sample"
						>
							<ArrowLeft size={18} />
						</button>
						<button
							type="button"
							className="absolute right-3 top-1/2 -translate-y-1/2 rounded-full bg-black/60 p-2 text-white transition hover:bg-black/80 disabled:opacity-30"
							onClick={() => moveSelection(1)}
							disabled={!findAdjacentPreviewFile(files, selectedFile.id, 1)}
							aria-label="Next preview sample"
						>
							<ArrowRight size={18} />
						</button>
					</>
				)}
			</div>
			<DirectoryPreview
				file={directory}
				previewFiles={files}
				onPreviewFile={(file) => setSelectedFileId(file.id)}
				selectedPreviewFileId={selectedFile?.id ?? null}
				previewLabel={previewSequenceLabel(
					files,
					selectedFile?.id ?? null,
					output?.candidate_budget_exhausted ?? false,
				)}
			/>
		</div>
	);
}

export function OrganizePreviewPane({
	taskId,
	selectedFile,
	siblingFiles = [],
	onSelectFile,
}: OrganizePreviewPaneProps) {
	const previousFile = useMemo(
		() =>
			selectedFile
				? findAdjacentPreviewFile(siblingFiles, selectedFile.id, -1)
				: null,
		[selectedFile, siblingFiles],
	);
	const nextFile = useMemo(
		() =>
			selectedFile
				? findAdjacentPreviewFile(siblingFiles, selectedFile.id, 1)
				: null,
		[selectedFile, siblingFiles],
	);

	const moveItem = useCallback(
		(file: File | null) => {
			if (file) onSelectFile?.(file);
		},
		[onSelectFile],
	);

	if (!selectedFile) {
		return (
			<div className="flex h-full items-center justify-center p-6 text-center text-sm text-ink-dull">
				Select an item to preview it.
			</div>
		);
	}

	return (
		<div
			className="flex h-full min-h-0 flex-col bg-app-box/30 outline-none"
			tabIndex={0}
			onKeyDown={(event) => {
				if (event.key === "ArrowLeft") {
					event.preventDefault();
					moveItem(previousFile);
				} else if (event.key === "ArrowRight") {
					event.preventDefault();
					moveItem(nextFile);
				}
			}}
			aria-label="Organize preview"
		>
			<div className="flex shrink-0 items-center gap-2 border-b border-app-line px-3 py-2">
				<button
					type="button"
					className="rounded-md p-1.5 text-ink-dull transition hover:bg-app-hover hover:text-ink disabled:cursor-not-allowed disabled:opacity-30"
					onClick={() => moveItem(previousFile)}
					disabled={!previousFile || !onSelectFile}
					aria-label="Previous item"
				>
					<ArrowLeft size={16} />
				</button>
				<div className="min-w-0 flex-1 text-center">
					<div className="truncate text-sm text-ink" title={selectedFile.name}>
						{selectedFile.name}
					</div>
					{siblingFiles.length > 0 && (
						<div className="text-[11px] text-ink-faint">
							{Math.max(
								0,
								siblingFiles.findIndex(
									(file) => file.id === selectedFile.id,
								) + 1,
							)} / {siblingFiles.length}
						</div>
					)}
				</div>
				<button
					type="button"
					className="rounded-md p-1.5 text-ink-dull transition hover:bg-app-hover hover:text-ink disabled:cursor-not-allowed disabled:opacity-30"
					onClick={() => moveItem(nextFile)}
					disabled={!nextFile || !onSelectFile}
					aria-label="Next item"
				>
					<ArrowRight size={16} />
				</button>
			</div>
			<div className="min-h-0 flex-1">
				{selectedFile.kind === "Directory" ? (
					<PreviewSequence
						directory={selectedFile}
						taskId={taskId}
						itemId={selectedFile.id}
					/>
				) : (
					<div className="h-full bg-black">
						<ContentRenderer
							key={selectedFile.id}
							file={selectedFile}
							videoKeyboardShortcutsEnabled={false}
							videoWheelZoomEnabled={false}
						/>
					</div>
				)}
			</div>
			<div className="flex shrink-0 items-center gap-2 border-t border-app-line px-3 py-2 text-xs text-ink-faint">
				<FileComponent.Thumb file={selectedFile} size={28} />
				<span className="truncate">Preview is read-only and does not change the task decision.</span>
			</div>
		</div>
	);
}
