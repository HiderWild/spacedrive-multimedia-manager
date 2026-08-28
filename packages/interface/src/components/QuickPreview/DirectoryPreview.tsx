import { useMemo } from "react";
import type {
	DirectoryListingInput,
	DirectoryListingOutput,
	File,
} from "@sd/ts-client";
import { File as FileComponent } from "../../routes/explorer/File";
import { useNormalizedQuery } from "../../contexts/SpacedriveContext";
import { Folder } from "@sd/assets/icons";

interface DirectoryPreviewProps {
	file: File;
	/**
	 * Optional bounded candidates supplied by a fixed organize snapshot.
	 * When present, the live directory query is disabled.
	 */
	previewFiles?: File[];
	onPreviewFile?: (file: File) => void;
	selectedPreviewFileId?: string | null;
	previewLabel?: string;
}

export function DirectoryPreview({
	file,
	previewFiles,
	onPreviewFile,
	selectedPreviewFileId,
	previewLabel,
}: DirectoryPreviewProps) {
	const directoryQuery = useNormalizedQuery<
		DirectoryListingInput,
		DirectoryListingOutput
	>({
		query: "files.directory_listing",
		input: {
			path: file.sd_path,
			limit: null,
			include_hidden: false,
			sort_by: "modified",
			folders_first: true,
		},
		resourceType: "file",
		pathScope: file.sd_path,
		enabled: previewFiles === undefined,
	});

	const allFiles = previewFiles ?? directoryQuery.data?.files ?? [];

	const directories = useMemo(() => {
		return allFiles;
	}, [allFiles]);

	const gridSize = 120;
	const gapSize = 12;

	if (previewFiles === undefined && directoryQuery.isLoading) {
		return (
			<div className="w-full h-full flex items-center justify-center">
				<div className="text-center">
					<img
						src={Folder}
						alt="Folder Icon"
						className="w-16 h-16 mb-4 mx-auto"
					/>
					<div className="text-lg font-medium text-ink">
						{file.name}
					</div>
					<div className="text-sm text-ink-dull mt-2">
						Loading directories...
					</div>
				</div>
			</div>
		);
	}

	if (directories.length === 0) {
		return (
			<div className="w-full h-full flex items-center justify-center">
				<div className="text-center">
					<img
						src={Folder}
						alt="Folder Icon"
						className="w-16 h-16 mb-4 mx-auto"
					/>
					<div className="text-lg font-medium text-ink">
						{file.name}
					</div>
					<div className="text-sm text-ink-dull mt-2">
						{previewFiles === undefined
							? "No subdirectories"
							: "No image or video samples"}
					</div>
				</div>
			</div>
		);
	}

	const thumbSize = Math.max(gridSize * 0.6, 60);

	return (
		<div className="w-full h-full overflow-auto">
			<div
				className="grid p-6"
				style={{
					gridTemplateColumns: `repeat(auto-fill, minmax(${gridSize}px, 1fr))`,
					gridAutoRows: "max-content",
					gap: `${gapSize}px`,
				}}
			>
				{directories.map((dir: File) => (
					<button
						type="button"
						key={dir.id}
						onClick={() => onPreviewFile?.(dir)}
						className={`flex flex-col items-center gap-2 rounded-lg p-1 text-left transition-colors hover:bg-app-hover/20 ${
							selectedPreviewFileId === dir.id
								? "bg-sidebar-selected/70 ring-1 ring-accent/60"
								: ""
						}`}
						aria-label={`Preview ${dir.name}`}
						aria-pressed={selectedPreviewFileId === dir.id}
					>
						<div className="rounded-lg p-2">
							<FileComponent.Thumb file={dir} size={thumbSize} />
						</div>
						<div className="w-full flex flex-col items-center">
							<div className="text-sm truncate px-2 py-0.5 rounded-md inline-block max-w-full text-ink">
								{dir.name}
							</div>
						</div>
					</button>
				))}
			</div>
			{previewLabel && (
				<div className="px-6 pb-4 text-center text-xs text-ink-faint">
					{previewLabel}
				</div>
			)}
		</div>
	);
}
