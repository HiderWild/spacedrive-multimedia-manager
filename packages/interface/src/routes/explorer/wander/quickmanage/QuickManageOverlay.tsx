import { useState } from "react";
import {
	DotsThree,
	Heart,
	FolderOpen,
	Tag as TagIcon,
	Trash,
} from "@phosphor-icons/react";
import clsx from "clsx";
import { toast } from "@spacedrive/primitives";
import type { File, SdPath, Tag } from "@sd/ts-client";
import {
	useLibraryMutation,
	useNormalizedQuery,
} from "../../../../contexts/SpacedriveContext";
import { useRefetchTagQueries } from "../../../../hooks/useRefetchTagQueries";
import { useFileOperationDialog } from "../../../../components/modals/FileOperationModal";
import { useDeleteFiles } from "../../hooks/useDeleteFiles";

interface QuickManageOverlayProps {
	/** The pane's current item, or null when the set is empty. */
	file: File | null;
	/**
	 * Advance the pane to the next item. Called after a successful mutating
	 * action (tag/move/delete) so the just-acted item leaves the view and the
	 * library refetch reflects the change.
	 */
	onAdvance: () => void;
}

/**
 * Hover/focus quick-management overlay for a single wander pane.
 *
 * Surfaces the same canonical file operations the explorer's right-click menu
 * uses — `tags.apply` (via the shared tag palette), move (the `files.copy`
 * file-operation dialog with `move_files`), and `files.delete` (the shared
 * `useDeleteFiles` hook). After a successful mutation the pane advances so the
 * acted item is replaced and the underlying query refetch updates the library.
 *
 * Favorite has no backend op yet (`FileInspector` carries the same
 * `metadata.set_favorite` TODO), so the heart is a local visual toggle only and
 * never advances; it is wired here so the affordance is in place once the op
 * lands.
 */
export function QuickManageOverlay({ file, onAdvance }: QuickManageOverlayProps) {
	const refetchTagQueries = useRefetchTagQueries();
	const applyTag = useLibraryMutation("tags.apply", {
		onSuccess: refetchTagQueries,
	});
	const openFileOperation = useFileOperationDialog();
	const { deleteFiles } = useDeleteFiles();

	const [tagPickerOpen, setTagPickerOpen] = useState(false);
	const [favorited, setFavorited] = useState(false);

	// Palette of tags to quick-apply, mirroring TagAssignmentMode's source.
	const { data: tagsData } = useNormalizedQuery<
		{ query: string },
		{ tags: Array<{ tag: Tag } | Tag> }
	>({
		query: "tags.search",
		input: { query: "" },
		resourceType: "tag",
	});

	const paletteTags: Tag[] = (
		tagsData?.tags?.map((result) => ("tag" in result ? result.tag : result)) ??
		[]
	).slice(0, 12);

	if (!file) return null;
	const currentFile = file;

	const handleApplyTag = async (tag: Tag) => {
		const contentId = currentFile.content_identity?.uuid;
		if (!contentId) {
			toast.error("This file needs to be indexed before it can be tagged");
			return;
		}
		try {
			await applyTag.mutateAsync({
				targets: { type: "Content", ids: [contentId] },
				tag_ids: [tag.id],
				source: null,
				confidence: null,
				applied_context: null,
				instance_attributes: null,
			});
			setTagPickerOpen(false);
			onAdvance();
		} catch (err) {
			console.error("Failed to apply tag:", err);
			toast.error(`Failed to apply tag: ${err}`);
		}
	};

	const handleMove = () => {
		const source = currentFile.sd_path;
		if (!("Physical" in source)) {
			toast.error("Only physical files can be moved");
			return;
		}
		const dest = window.prompt("Move to directory (absolute path):");
		if (!dest) return;
		const destination: SdPath = {
			Physical: { device_slug: source.Physical.device_slug, path: dest },
		};
		openFileOperation({
			operation: "move",
			sources: [source],
			destination,
			onComplete: onAdvance,
		});
	};

	const handleDelete = async () => {
		const deleted = await deleteFiles([currentFile], false);
		if (deleted) onAdvance();
	};

	return (
		<div className="absolute right-3 top-3 z-10 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100">
			<div className="relative flex items-center gap-1.5 rounded-full bg-black/55 px-2 py-1.5 backdrop-blur">
				<QuickButton
					label="Quick tag"
					onClick={() => setTagPickerOpen((open) => !open)}
					active={tagPickerOpen}
				>
					<TagIcon size={16} weight="bold" />
				</QuickButton>
				<QuickButton label="Move" onClick={handleMove}>
					<FolderOpen size={16} weight="bold" />
				</QuickButton>
				<QuickButton
					label={favorited ? "Unfavorite" : "Favorite"}
					onClick={() => setFavorited((value) => !value)}
					active={favorited}
				>
					<Heart size={16} weight={favorited ? "fill" : "bold"} />
				</QuickButton>
				<QuickButton label="Delete" onClick={handleDelete} danger>
					<Trash size={16} weight="bold" />
				</QuickButton>

				{tagPickerOpen && (
					<div className="absolute right-0 top-full mt-2 max-h-64 w-48 overflow-y-auto rounded-lg bg-black/85 p-1.5 shadow-xl backdrop-blur">
						{paletteTags.length === 0 ? (
							<div className="flex items-center gap-2 px-2 py-2 text-xs text-white/50">
								<DotsThree size={16} />
								No tags yet
							</div>
						) : (
							paletteTags.map((tag) => {
								const applied = currentFile.tags?.some(
									(t) => t.id === tag.id,
								);
								return (
									<button
										key={tag.id}
										type="button"
										onClick={() => handleApplyTag(tag)}
										className={clsx(
											"flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition",
											applied
												? "bg-accent/30 text-white"
												: "text-white/80 hover:bg-white/10 hover:text-white",
										)}
									>
										<span
											className="h-2.5 w-2.5 shrink-0 rounded-full"
											style={{ backgroundColor: tag.color ?? "#888" }}
										/>
										<span className="truncate">
											{tag.display_name ?? tag.canonical_name}
										</span>
									</button>
								);
							})
						)}
					</div>
				)}
			</div>
		</div>
	);
}

interface QuickButtonProps {
	label: string;
	onClick: () => void;
	active?: boolean;
	danger?: boolean;
	children: React.ReactNode;
}

function QuickButton({ label, onClick, active, danger, children }: QuickButtonProps) {
	return (
		<button
			type="button"
			aria-label={label}
			title={label}
			onClick={onClick}
			className={clsx(
				"flex h-8 w-8 items-center justify-center rounded-full transition",
				danger
					? "bg-white/10 text-white/80 hover:bg-red-500/80 hover:text-white"
					: active
						? "bg-accent text-white"
						: "bg-white/10 text-white/80 hover:bg-white/20 hover:text-white",
			)}
		>
			{children}
		</button>
	);
}
