import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { File } from "@sd/ts-client";
import { useNormalizedQuery } from "../../../contexts/SpacedriveContext";
import { usePlatform } from "../../../contexts/PlatformContext";
import { useExplorer } from "../context";
import { File as FileComponent } from "../File";
import { VideoPlayer } from "../../../components/QuickPreview/VideoPlayer";
import { getPhysicalPath } from "./organizePersistence";
import { deriveDirectoryPreviewAvailability, toMediaSortBy, toPreviewListSortBy } from "./organizePreview";
import type { OrganizePreviewTab } from "./organizeTypes";

const VIDEO_EXTENSIONS = /^(mp4|mov|mkv|webm|avi)$/i;
const IMAGE_EXTENSIONS = /^(png|jpe?g|gif|webp|bmp|svg)$/i;

export function OrganizePreviewPane(props: { selectedFile: File | null }) {
	const { t } = useTranslation("explorer");
	const platform = usePlatform();
	const { sortBy, viewSettings } = useExplorer();
	const [activeTab, setActiveTab] = useState<OrganizePreviewTab>("list");
	const selectedDirectory = props.selectedFile?.kind === "Directory" ? props.selectedFile : null;

	const mediaQuery = useNormalizedQuery({
		query: "files.media_listing",
		input: selectedDirectory?.sd_path
			? {
					path: selectedDirectory.sd_path,
					include_descendants: true,
					media_types: null,
					limit: 10000,
					sort_by: toMediaSortBy(sortBy),
				}
			: null!,
		resourceType: "file",
		pathScope: selectedDirectory?.sd_path ?? undefined,
		includeDescendants: true,
		enabled: !!selectedDirectory,
	});

	const listQuery = useNormalizedQuery({
		query: "files.directory_listing",
		input: selectedDirectory?.sd_path
			? {
					path: selectedDirectory.sd_path,
					limit: null,
					include_hidden: false,
					sort_by: toPreviewListSortBy(sortBy),
					folders_first: viewSettings.foldersFirst,
				}
			: null!,
		resourceType: "file",
		pathScope: selectedDirectory?.sd_path ?? undefined,
		enabled: !!selectedDirectory,
	});

	const mediaFiles = (mediaQuery.data as { files: File[] } | undefined)?.files ?? [];
	const listFiles = (listQuery.data as { files: File[] } | undefined)?.files ?? [];

	const availability = useMemo(() => deriveDirectoryPreviewAvailability(mediaFiles), [mediaFiles]);

	useEffect(() => {
		setActiveTab(selectedDirectory ? availability.defaultTab : "list");
	}, [availability.defaultTab, selectedDirectory?.id]);

	if (!props.selectedFile) {
		return (
			<div className="flex h-full items-center justify-center p-4 text-sm text-ink-dull">
				{t("organize.previewEmpty")}
			</div>
		);
	}

	// Single file preview: video or image when supported
	if (props.selectedFile.kind === "File") {
		const path = getPhysicalPath(props.selectedFile.sd_path);
		const src = path && platform.convertFileSrc ? platform.convertFileSrc(path) : null;

		if (!src) {
			return (
				<div className="flex h-full items-center justify-center p-4 text-sm text-ink-dull">
					{t("organize.previewEmpty")}
				</div>
			);
		}

		if (props.selectedFile.extension && VIDEO_EXTENSIONS.test(props.selectedFile.extension)) {
			return <VideoPlayer src={src} file={props.selectedFile} />;
		}

		if (props.selectedFile.extension && IMAGE_EXTENSIONS.test(props.selectedFile.extension)) {
			return (
				<img
					src={src}
					alt={props.selectedFile.name}
					className="h-full w-full bg-black object-contain"
				/>
			);
		}

		// Unsupported file type for preview
		return (
			<div className="flex h-full items-center justify-center p-4 text-sm text-ink-dull">
				{t("organize.previewEmpty")}
			</div>
		);
	}

	// Directory preview: tabs for video / image / list
	const previewFile = activeTab === "video" ? availability.firstVideo : availability.firstImage;
	const previewPath = previewFile ? getPhysicalPath(previewFile.sd_path) : null;
	const previewSrc = previewPath && platform.convertFileSrc ? platform.convertFileSrc(previewPath) : null;

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex gap-1 border-b border-app-line p-2">
				{availability.renderedTabs.map((tab) => {
					const enabled = availability.enabledTabs.includes(tab);
					const label = tab === "list" ? t("organize.previewList") : tab === "video" ? t("organize.previewVideo") : t("organize.previewImage");
					const tooltip = !enabled
						? tab === "video"
							? t("organize.previewMissingVideo")
							: t("organize.previewMissingImage")
						: undefined;
					return (
						<button
							key={tab}
							disabled={!enabled}
							onClick={() => enabled && setActiveTab(tab)}
							title={tooltip}
							aria-label={tooltip ?? label}
							className={`rounded-md px-3 py-2 text-sm ${activeTab === tab ? "bg-accent/15 text-accent" : "text-ink-dull hover:bg-app-box"} disabled:cursor-not-allowed disabled:opacity-40`}
						>
							{label}
						</button>
					);
				})}
			</div>
			<div className="min-h-0 flex-1 overflow-auto">
				{activeTab === "list" ? (
					<div className="flex flex-col gap-2 p-3">
						{listFiles.map((file) => (
							<div
								key={file.id}
								className="flex items-center gap-3 rounded-lg border border-app-line p-2"
							>
								<FileComponent.Thumb file={file} size={40} />
								<div className="truncate text-sm text-ink">{file.name}</div>
							</div>
						))}
					</div>
				) : previewFile && previewSrc ? (
					activeTab === "video" ? (
						<VideoPlayer src={previewSrc} file={previewFile} />
					) : (
						<img
							src={previewSrc}
							alt={previewFile.name}
							className="h-full w-full bg-black object-contain"
						/>
					)
				) : (
					<div className="flex h-full items-center justify-center p-4 text-sm text-ink-dull">
						{t("organize.previewEmpty")}
					</div>
				)}
			</div>
		</div>
	);
}
