import {CheckCircle, XCircle, CaretDown, CaretUp} from '@phosphor-icons/react';
import type {File} from '@sd/ts-client';
import clsx from 'clsx';
import {useCallback, useEffect, useRef, useState, type WheelEvent} from 'react';
import {useTranslation} from 'react-i18next';
import {OrganizeThumbnail} from './OrganizeThumbnail';
import type {OrganizePresentationEntry} from './organizeState';
import type {OrganizeCenterLayout} from './organizeTypes';

export function OrganizeCenterPane(props: {
	layout: OrganizeCenterLayout;
	onLayoutChange: (layout: OrganizeCenterLayout) => void;
	presentation: OrganizePresentationEntry[];
	selectedFileId: string | null;
	multiSelectedIds: Set<string>;
	onSelectFile: (file: File, isMulti?: boolean) => void;
	onToggleMultiSelect: (fileId: string) => void;
	onClearMultiSelect: () => void;
	onMarkKeep: (file: File) => void;
	onMarkDiscard: (file: File) => void;
	onClearDecision: (file: File) => void;
	onNavigateToDirectory?: (file: File) => void;
	onLoadMore?: () => void;
	hasMore?: boolean;
}) {
	const {t} = useTranslation('explorer');
	const [itemSize, setItemSize] = useState(140);
	const [showDecisionBar, setShowDecisionBar] = useState(true);
	const containerRef = useRef<HTMLDivElement>(null);
	const scrollContainerRef = useRef<HTMLDivElement>(null);
	const [isSelecting, setIsSelecting] = useState(false);
	const [selectionStart, setSelectionStart] = useState<{x: number; y: number} | null>(null);
	const [selectionEnd, setSelectionEnd] = useState<{x: number; y: number} | null>(null);

	const selected =
		props.presentation.find((item) => item.file.id === props.selectedFileId)
			?.file ?? null;

	const handleWheel = useCallback((e: WheelEvent) => {
		if (!e.ctrlKey) return;
		e.preventDefault();
		const delta = e.deltaY > 0 ? -20 : 20;

		if (props.layout === 'list') {
			// List view: zoom in (delta > 0) switches to minimum grid
			if (delta > 0) {
				props.onLayoutChange('grid');
				setItemSize(80);
			}
			// Zoom out in list view does nothing (already at minimum)
		} else {
			// Grid view: normal zoom behavior
			const newSize = Math.max(80, Math.min(240, itemSize + delta));

			// Auto-switch to list view when trying to zoom below minimum
			if (newSize === 80 && delta < 0) {
				props.onLayoutChange('list');
			} else {
				setItemSize(newSize);
			}
		}
	}, [itemSize, props]);

	// Handle keyboard shortcuts
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			// Delete key: mark as discard
			if (e.key === 'Delete' && selected && !e.repeat) {
				e.preventDefault();
				props.onMarkDiscard(selected);
			}
			// Enter key: navigate into directory
			else if (e.key === 'Enter' && selected && !e.repeat) {
				if (selected.kind === 'Directory' && props.onNavigateToDirectory) {
					e.preventDefault();
					props.onNavigateToDirectory(selected);
				}
			}
			// Backspace key: navigate back (handled globally, but we can trigger it here too)
			else if (e.key === 'Backspace' && !e.repeat) {
				e.preventDefault();
				// Backspace should trigger global back navigation
				// This will be handled by adding a global listener
			}
		};

		const container = containerRef.current;
		if (container) {
			container.addEventListener('keydown', handleKeyDown);
			return () => container.removeEventListener('keydown', handleKeyDown);
		}
	}, [selected, props]);

	// Calculate icon size proportionally to item size (65% of item size)
	const iconSize = Math.floor(itemSize * 0.65);

	// Infinite scroll: load more when near bottom
	useEffect(() => {
		const scrollContainer = scrollContainerRef.current;
		if (!scrollContainer || !props.hasMore || !props.onLoadMore) return;

		const handleScroll = () => {
			const { scrollTop, scrollHeight, clientHeight } = scrollContainer;
			const scrollPercentage = (scrollTop + clientHeight) / scrollHeight;

			// Load more when scrolled past 80%
			if (scrollPercentage > 0.8) {
				props.onLoadMore?.();
			}
		};

		scrollContainer.addEventListener('scroll', handleScroll);
		return () => scrollContainer.removeEventListener('scroll', handleScroll);
	}, [props]);

	// Rectangle selection (lasso)
	useEffect(() => {
		const scrollContainer = scrollContainerRef.current;
		if (!scrollContainer) return;

		const handleMouseDown = (e: MouseEvent) => {
			// Only start selection on left click on the container background
			if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return;

			setIsSelecting(true);
			const rect = scrollContainer.getBoundingClientRect();
			setSelectionStart({ x: e.clientX - rect.left, y: e.clientY - rect.top + scrollContainer.scrollTop });
			setSelectionEnd({ x: e.clientX - rect.left, y: e.clientY - rect.top + scrollContainer.scrollTop });
		};

		const handleMouseMove = (e: MouseEvent) => {
			if (!isSelecting || !selectionStart) return;

			const rect = scrollContainer.getBoundingClientRect();
			setSelectionEnd({ x: e.clientX - rect.left, y: e.clientY - rect.top + scrollContainer.scrollTop });
		};

		const handleMouseUp = () => {
			if (!isSelecting || !selectionStart || !selectionEnd) {
				setIsSelecting(false);
				return;
			}

			// Calculate selection rectangle
			const rect = {
				left: Math.min(selectionStart.x, selectionEnd.x),
				right: Math.max(selectionStart.x, selectionEnd.x),
				top: Math.min(selectionStart.y, selectionEnd.y),
				bottom: Math.max(selectionStart.y, selectionEnd.y),
			};

			// Find items within selection rectangle
			const buttons = scrollContainer.querySelectorAll('button[data-file-id]');
			buttons.forEach((button) => {
				const buttonRect = button.getBoundingClientRect();
				const containerRect = scrollContainer.getBoundingClientRect();
				const relativeTop = buttonRect.top - containerRect.top + scrollContainer.scrollTop;
				const relativeLeft = buttonRect.left - containerRect.left;

				// Check if button intersects with selection
				const intersects =
					relativeLeft < rect.right &&
					relativeLeft + buttonRect.width > rect.left &&
					relativeTop < rect.bottom &&
					relativeTop + buttonRect.height > rect.top;

				if (intersects) {
					const fileId = button.getAttribute('data-file-id');
					if (fileId) {
						props.onToggleMultiSelect(fileId);
					}
				}
			});

			setIsSelecting(false);
			setSelectionStart(null);
			setSelectionEnd(null);
		};

		scrollContainer.addEventListener('mousedown', handleMouseDown);
		document.addEventListener('mousemove', handleMouseMove);
		document.addEventListener('mouseup', handleMouseUp);

		return () => {
			scrollContainer.removeEventListener('mousedown', handleMouseDown);
			document.removeEventListener('mousemove', handleMouseMove);
			document.removeEventListener('mouseup', handleMouseUp);
		};
	}, [isSelecting, selectionStart, selectionEnd, props]);

	return (
		<div ref={containerRef} className="flex h-full min-h-0 flex-col" tabIndex={-1}>
			{/* Decision bar with toggle */}
			<div className="border-app-line border-b">
				{showDecisionBar && (
					<div className="flex items-center gap-2 px-3 py-2">
						<button
							className="rounded-md bg-emerald-500/15 px-3 py-1.5 text-sm text-emerald-300 disabled:opacity-40"
							disabled={!selected}
							onClick={() => selected && props.onMarkKeep(selected)}
						>
							Keep
						</button>
						<button
							className="rounded-md bg-rose-500/15 px-3 py-1.5 text-sm text-rose-300 disabled:opacity-40"
							disabled={!selected}
							onClick={() => selected && props.onMarkDiscard(selected)}
						>
							Discard
						</button>
						<button
							className="bg-app-box text-ink rounded-md px-3 py-1.5 text-sm disabled:opacity-40"
							disabled={!selected}
							onClick={() => selected && props.onClearDecision(selected)}
						>
							Clear
						</button>
					</div>
				)}
				{/* Toggle button */}
				<button
					onClick={() => setShowDecisionBar(!showDecisionBar)}
					className="text-ink-dull hover:text-ink hover:bg-app-darkBox flex w-full items-center justify-center gap-1 py-1 text-xs transition-colors"
					title={showDecisionBar ? 'Hide decision bar' : 'Show decision bar'}
				>
					{showDecisionBar ? (
						<>
							<CaretUp size={12} weight="bold" />
							<span>Hide</span>
						</>
					) : (
						<>
							<CaretDown size={12} weight="bold" />
							<span>Show Actions</span>
						</>
					)}
				</button>
			</div>
			<div
				ref={scrollContainerRef}
				className={clsx(
					'min-h-0 flex-1 overflow-auto p-3',
					props.layout === 'grid'
						? 'grid gap-3'
						: 'flex flex-col gap-1'
				)}
				style={
					props.layout === 'grid'
						? {
								gridTemplateColumns: `repeat(auto-fill, minmax(${itemSize}px, 1fr))`,
								gridAutoRows: `${itemSize + 40}px`, // Control row height
						  }
						: undefined
				}
				onWheel={handleWheel}
				onClick={(e) => {
					// Clear multi-selection on blank area click
					if (e.target === scrollContainerRef.current) {
						props.onClearMultiSelect();
					}
				}}
			>
				{props.layout === 'list' ? (
					// List view
					<>
						{props.presentation.map((item) => {
							const isMultiSelected = props.multiSelectedIds.has(item.file.id);
							const isSingleSelected = item.file.id === props.selectedFileId;

							return (
								<button
									key={item.file.id}
									data-file-id={item.file.id}
									onClick={(e) => {
										if (e.ctrlKey || e.metaKey) {
											props.onToggleMultiSelect(item.file.id);
										} else {
											props.onSelectFile(item.file, false);
										}
									}}
									onDoubleClick={() => {
										if (item.file.kind === 'Directory' && props.onNavigateToDirectory) {
											props.onNavigateToDirectory(item.file);
										}
									}}
									className={clsx(
										'border-app-line bg-app-box/60 flex items-center gap-3 rounded-lg border px-3 py-2 text-left',
										item.dimmed && 'opacity-50',
										(isMultiSelected || isSingleSelected) && 'ring-accent ring-2'
									)}
								>
									<OrganizeThumbnail file={item.file} size={32} />
									<div className="text-ink min-w-0 flex-1 truncate text-sm">
										{item.file.name}
									</div>
									<div className="text-ink-dull text-xs">
										{item.file.size ? formatBytes(item.file.size) : '—'}
									</div>
									<div className="text-ink-dull text-xs">
										{item.file.date_modified ? formatDate(item.file.date_modified) : '—'}
									</div>
									{item.decision === 'keep' ? (
										<CheckCircle
											className="text-emerald-400 shrink-0"
											size={18}
											weight="fill"
										/>
									) : item.decision === 'discard' ? (
										<XCircle
											className="text-rose-400 shrink-0"
											size={18}
											weight="fill"
										/>
									) : null}
								</button>
							);
						})}
					</>
				) : (
					// Grid view
					<>
						{props.presentation.map((item) => {
							const isMultiSelected = props.multiSelectedIds.has(item.file.id);
							const isSingleSelected = item.file.id === props.selectedFileId;

							return (
								<button
									key={item.file.id}
									data-file-id={item.file.id}
									onClick={(e) => {
										if (e.ctrlKey || e.metaKey) {
											props.onToggleMultiSelect(item.file.id);
										} else {
											props.onSelectFile(item.file, false);
										}
									}}
									onDoubleClick={() => {
										if (item.file.kind === 'Directory' && props.onNavigateToDirectory) {
											props.onNavigateToDirectory(item.file);
										}
									}}
									className={clsx(
										'border-app-line bg-app-box/60 relative flex flex-col items-center justify-center rounded-xl border p-2',
										item.dimmed && 'opacity-50',
										(isMultiSelected || isSingleSelected) && 'ring-accent ring-2'
									)}
								>
									<OrganizeThumbnail
										file={item.file}
										size={props.layout === 'grid' ? iconSize : 48}
									/>
									<div className="text-ink mt-1 w-full truncate text-center text-xs">
										{item.file.name}
									</div>
									{item.decision === 'keep' ? (
										<CheckCircle
											className="absolute bottom-2 right-2 text-emerald-400"
											size={20}
											weight="fill"
										/>
									) : item.decision === 'discard' ? (
										<XCircle
											className="absolute bottom-2 right-2 text-rose-400"
											size={20}
											weight="fill"
										/>
									) : null}
								</button>
							);
						})}
					</>
				)}

				{/* Selection rectangle visualization */}
				{isSelecting && selectionStart && selectionEnd && (
					<div
						className="border-accent pointer-events-none absolute z-50 border-2 bg-accent/10"
						style={{
							left: Math.min(selectionStart.x, selectionEnd.x),
							top: Math.min(selectionStart.y, selectionEnd.y),
							width: Math.abs(selectionEnd.x - selectionStart.x),
							height: Math.abs(selectionEnd.y - selectionStart.y),
						}}
					/>
				)}
			</div>
		</div>
	);
}

// Helper functions
function formatBytes(bytes: number): string {
	if (bytes === 0) return '0 B';
	const k = 1024;
	const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
}

function formatDate(date: string): string {
	const d = new Date(date);
	return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}
