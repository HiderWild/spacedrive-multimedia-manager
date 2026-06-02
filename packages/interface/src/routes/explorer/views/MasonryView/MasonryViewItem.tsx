import clsx from "clsx";
import { memo } from "react";
import type { File } from "@sd/ts-client";
import { File as FileComponent } from "../../File";
import { useSelection } from "../../SelectionContext";
import { useFileContextMenu } from "../../hooks/useFileContextMenu";

function formatDuration(seconds: number): string {
	const mins = Math.floor(seconds / 60);
	const secs = Math.floor(seconds % 60);
	return `${mins}:${String(secs).padStart(2, "0")}`;
}

interface MasonryViewItemProps {
	file: File;
	allFiles: File[];
	selected: boolean;
	focused: boolean;
	onSelect: (
		file: File,
		files: File[],
		multi?: boolean,
		range?: boolean,
	) => void;
	width: number;
	height: number;
}

/**
 * A single justified-layout tile.
 *
 * The wrapper is sized to the exact box the layout solver produced, which
 * already matches the file's aspect ratio, so the thumbnail uses
 * `object-contain` with `squareMode={false}` to fill the box edge-to-edge with
 * no cropping and no overflow.
 */
export const MasonryViewItem = memo(function MasonryViewItem({
	file,
	allFiles,
	selected,
	focused,
	onSelect,
	width,
	height,
}: MasonryViewItemProps) {
	const { selectedFiles } = useSelection();

	const contextMenu = useFileContextMenu({
		file,
		selectedFiles,
		selected,
	});

	const handleClick = (e: React.MouseEvent) => {
		const multi = e.metaKey || e.ctrlKey;
		const range = e.shiftKey;
		onSelect(file, allFiles, multi, range);
	};

	const handleContextMenu = async (e: React.MouseEvent) => {
		e.preventDefault();
		e.stopPropagation();

		if (!selected) {
			onSelect(file, allFiles, false, false);
		}

		await contextMenu.show(e);
	};

	// Thumbnail resolution scales with the rendered box's largest edge.
	const thumbSize = Math.round(Math.max(width, height));

	return (
		<div
			data-file-id={file.id}
			data-selectable="true"
			tabIndex={-1}
			className={clsx(
				"relative overflow-hidden cursor-pointer rounded-md bg-app-darkBox transition-all group outline-none focus:outline-none",
				selected && "ring-2 ring-accent ring-inset",
				focused && !selected && "ring-2 ring-accent/50 ring-inset",
			)}
			style={{ width, height }}
			onClick={handleClick}
			onContextMenu={handleContextMenu}
		>
			<FileComponent.Thumb
				file={file}
				size={thumbSize}
				className="w-full h-full"
				frameClassName="w-full h-full object-contain"
				iconScale={0.5}
				squareMode={false}
			/>

			{selected && (
				<div className="absolute inset-0 bg-accent/10 pointer-events-none" />
			)}

			{file.video_media_data?.duration_seconds && (
				<div className="absolute bottom-1 right-1 px-1.5 py-0.5 rounded bg-black/80 text-white text-[10px] font-medium backdrop-blur-sm tabular-nums">
					{formatDuration(file.video_media_data.duration_seconds)}
				</div>
			)}
		</div>
	);
});
