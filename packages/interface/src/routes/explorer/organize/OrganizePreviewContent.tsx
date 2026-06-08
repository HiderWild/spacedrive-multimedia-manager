import {ArrowSquareOut} from '@phosphor-icons/react';
import {getContentKind, type File} from '@sd/ts-client';
import {
	type WheelEvent as ReactWheelEvent,
	useCallback,
	useEffect,
	useMemo,
	useState
} from 'react';
import {useTranslation} from 'react-i18next';
import {ContentRenderer} from '../../../components/QuickPreview/ContentRenderer';
import type {VideoControlsCallbacks} from '../../../components/QuickPreview/VideoControls';
import {usePlatform} from '../../../contexts/PlatformContext';
import {useNormalizedQuery} from '../../../contexts/SpacedriveContext';
import {isInputFocused} from '../../../util/keybinds/platform';
import {useExplorer} from '../context';
import {File as FileComponent} from '../File';
import {useSelection} from '../SelectionContext';
import {
	toMediaSortBy,
	toPreviewListSortBy,
	type OrganizeInspectorPreviewContext
} from './organizePreview';
import {
	filterPreviewCandidates,
	findAdjacentPreviewFile,
	getPreviewMediaKind
} from './organizePreviewMedia';
import type {OrganizePreviewTab} from './organizeTypes';

function scrollFileIntoView(fileId: string) {
	if (typeof document === 'undefined') return;
	document
		.querySelector<HTMLElement>(`[data-file-id="${fileId}"]`)
		?.scrollIntoView({block: 'nearest'});
}

export function OrganizePreviewContent(props: {
	selectedFile: File;
	activeTab: OrganizePreviewTab;
	context: OrganizeInspectorPreviewContext;
}) {
	const {t} = useTranslation('explorer');
	const platform = usePlatform();
	const {currentFiles, openQuickPreview} = useExplorer();
	const {selectFile} = useSelection();
	const [videoCallbacks, setVideoCallbacks] =
		useState<VideoControlsCallbacks | null>(null);
	const [directoryPreviewIndex, setDirectoryPreviewIndex] = useState(0);
	const selectedDirectory =
		props.selectedFile.kind === 'Directory' ? props.selectedFile : null;
	const previewMediaKind = getPreviewMediaKind(props.activeTab);

	const mediaQuery = useNormalizedQuery({
		query: 'files.media_listing',
		input: selectedDirectory?.sd_path
			? {
					path: selectedDirectory.sd_path,
					include_descendants: true,
					media_types: null,
					limit: 10000,
					sort_by: toMediaSortBy(props.context.sortBy)
				}
			: null!,
		resourceType: 'file',
		pathScope: selectedDirectory?.sd_path ?? undefined,
		includeDescendants: true,
		enabled: !!selectedDirectory
	});

	const listQuery = useNormalizedQuery({
		query: 'files.directory_listing',
		input: selectedDirectory?.sd_path
			? {
					path: selectedDirectory.sd_path,
					limit: null,
					include_hidden: false,
					sort_by: toPreviewListSortBy(props.context.sortBy),
					folders_first: props.context.foldersFirst
				}
			: null!,
		resourceType: 'file',
		pathScope: selectedDirectory?.sd_path ?? undefined,
		enabled: !!selectedDirectory && props.activeTab === 'list'
	});

	const mediaFiles =
		(mediaQuery.data as {files: File[]} | undefined)?.files ?? [];
	const listFiles =
		(listQuery.data as {files: File[]} | undefined)?.files ?? [];
	const directoryPreviewCandidates = useMemo(
		() =>
			previewMediaKind
				? filterPreviewCandidates(mediaFiles, previewMediaKind)
				: [],
		[mediaFiles, previewMediaKind]
	);
	const siblingPreviewCandidates = useMemo(
		() =>
			previewMediaKind
				? filterPreviewCandidates(currentFiles, previewMediaKind)
				: [],
		[currentFiles, previewMediaKind]
	);
	const previewFile = useMemo(() => {
		if (!previewMediaKind) {
			return null;
		}

		if (props.selectedFile.kind === 'Directory') {
			return directoryPreviewCandidates[directoryPreviewIndex] ?? null;
		}

		return getContentKind(props.selectedFile) === previewMediaKind
			? props.selectedFile
			: null;
	}, [
		directoryPreviewCandidates,
		directoryPreviewIndex,
		previewMediaKind,
		props.selectedFile
	]);

	useEffect(() => {
		setDirectoryPreviewIndex(0);
	}, [props.selectedFile.id, props.activeTab]);

	useEffect(() => {
		setVideoCallbacks(null);
	}, [previewFile?.id, props.activeTab]);

	const moveFileSelection = useCallback(
		(offset: -1 | 1) => {
			if (!previewMediaKind || props.selectedFile.kind !== 'File') {
				return;
			}

			const nextFile = findAdjacentPreviewFile({
				files: siblingPreviewCandidates,
				currentFileId: props.selectedFile.id,
				offset
			});
			if (!nextFile) {
				return;
			}

			selectFile(nextFile, currentFiles, false, false);
			scrollFileIntoView(nextFile.id);
		},
		[
			currentFiles,
			previewMediaKind,
			props.selectedFile,
			selectFile,
			siblingPreviewCandidates
		]
	);

	const moveDirectoryPreview = useCallback(
		(offset: -1 | 1) => {
			setDirectoryPreviewIndex((currentIndex) => {
				if (directoryPreviewCandidates.length === 0) {
					return 0;
				}

				return Math.min(
					directoryPreviewCandidates.length - 1,
					Math.max(0, currentIndex + offset)
				);
			});
		},
		[directoryPreviewCandidates.length]
	);

	const movePreview = useCallback(
		(offset: -1 | 1) => {
			if (props.selectedFile.kind === 'Directory') {
				moveDirectoryPreview(offset);
				return;
			}

			moveFileSelection(offset);
		},
		[moveDirectoryPreview, moveFileSelection, props.selectedFile.kind]
	);

	const openPreviewWindow = useCallback(() => {
		if (!previewFile) {
			return;
		}

		if (platform.showWindow) {
			void platform.showWindow({
				type: 'QuickPreview',
				file_id: previewFile.id
			});
			return;
		}

		openQuickPreview(previewFile.id);
	}, [openQuickPreview, platform, previewFile]);

	useEffect(() => {
		if (!previewMediaKind || !previewFile) {
			return;
		}

		const handleKeyDown = (event: KeyboardEvent) => {
			if (isInputFocused()) {
				return;
			}

			const stopEvent = () => {
				event.preventDefault();
				event.stopPropagation();
				event.stopImmediatePropagation();
			};

			if (previewMediaKind === 'image') {
				if (event.code === 'Space') {
					stopEvent();
					openPreviewWindow();
					return;
				}

				if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
					stopEvent();
					movePreview(-1);
					return;
				}

				if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
					stopEvent();
					movePreview(1);
				}

				return;
			}

			if (event.code === 'Space') {
				stopEvent();
				videoCallbacks?.onTogglePlay();
				return;
			}

			if (event.key === 'ArrowUp') {
				stopEvent();
				movePreview(-1);
				return;
			}

			if (event.key === 'ArrowDown') {
				stopEvent();
				movePreview(1);
				return;
			}

			if (event.key === 'ArrowLeft') {
				stopEvent();
				videoCallbacks?.onSeekBy(-5);
				return;
			}

			if (event.key === 'ArrowRight') {
				stopEvent();
				videoCallbacks?.onSeekBy(5);
			}
		};

		window.addEventListener('keydown', handleKeyDown, {capture: true});
		return () =>
			window.removeEventListener('keydown', handleKeyDown, {
				capture: true
			});
	}, [movePreview, openPreviewWindow, previewFile, previewMediaKind, videoCallbacks]);

	const handleVideoWheel = useCallback(
		(event: ReactWheelEvent<HTMLDivElement>) => {
			if (previewMediaKind !== 'video' || !videoCallbacks) {
				return;
			}

			if (
				(event.target as HTMLElement).closest(
					'button, input, textarea, [role="slider"]'
				)
			) {
				return;
			}

			event.preventDefault();
			event.stopPropagation();
			videoCallbacks.onStepFrames(event.deltaY > 0 ? 1 : -1);
		},
		[previewMediaKind, videoCallbacks]
	);

	const renderPreviewBody = (content: React.ReactNode) => (
		<div
			className="flex h-full min-h-0 flex-col"
			onWheel={previewMediaKind === 'video' ? handleVideoWheel : undefined}
		>
			<div className="min-h-0 flex-1">{content}</div>
		</div>
	);

	if (props.activeTab === 'list') {
		return renderPreviewBody(
			<div className="flex h-full min-h-0 flex-col">
				<div className="min-h-0 flex-1 overflow-auto">
					<div className="flex flex-col gap-2 p-3">
						{listFiles.map((file) => (
							<div
								key={file.id}
								className="border-app-line flex items-center gap-3 rounded-lg border p-2"
							>
								<FileComponent.Thumb file={file} size={40} />
								<div className="text-ink truncate text-sm">
									{file.name}
								</div>
							</div>
						))}
					</div>
				</div>
			</div>
		);
	}

	if (previewFile) {
		return renderPreviewBody(
			<>
				<div className="flex justify-end px-2 pt-2">
					<button
						type="button"
						onClick={openPreviewWindow}
						className="hover:bg-sidebar-selected inline-flex items-center gap-1.5 rounded-lg p-1.5 text-xs text-sidebar-inkDull transition-colors hover:text-sidebar-ink"
						title="Open Quick Preview"
					>
						<ArrowSquareOut className="size-4" weight="bold" />
					</button>
				</div>
				<div className="min-h-0 flex-1">
					<ContentRenderer
						file={previewFile}
						getVideoCallbacks={setVideoCallbacks}
						videoKeyboardShortcutsEnabled={false}
						videoWheelZoomEnabled={false}
					/>
				</div>
			</>
		);
	}

	return renderPreviewBody(
		<div className="text-ink-dull flex h-full items-center justify-center p-4 text-sm">
			{t('organize.previewEmpty')}
		</div>
	);
}
