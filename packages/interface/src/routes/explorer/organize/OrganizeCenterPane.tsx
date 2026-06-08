import {CheckCircle, XCircle} from '@phosphor-icons/react';
import type {File} from '@sd/ts-client';
import clsx from 'clsx';
import {useCallback, useState, type WheelEvent} from 'react';
import {useTranslation} from 'react-i18next';
import {File as FileComponent} from '../File';
import type {OrganizePresentationEntry} from './organizeState';
import type {OrganizeCenterLayout} from './organizeTypes';

export function OrganizeCenterPane(props: {
	layout: OrganizeCenterLayout;
	onLayoutChange: (layout: OrganizeCenterLayout) => void;
	presentation: OrganizePresentationEntry[];
	selectedFileId: string | null;
	onSelectFile: (file: File) => void;
	onMarkKeep: (file: File) => void;
	onMarkDiscard: (file: File) => void;
	onClearDecision: (file: File) => void;
	onNavigateToDirectory?: (file: File) => void;
}) {
	const {t} = useTranslation('explorer');
	const [itemSize, setItemSize] = useState(140);
	const selected =
		props.presentation.find((item) => item.file.id === props.selectedFileId)
			?.file ?? null;

	const handleWheel = useCallback((e: WheelEvent) => {
		if (!e.ctrlKey) return;
		e.preventDefault();
		const delta = e.deltaY > 0 ? -20 : 20;
		setItemSize((prev) => Math.max(80, Math.min(240, prev + delta)));
	}, []);

	// Calculate icon size proportionally to item size (65% of item size)
	const iconSize = Math.floor(itemSize * 0.65);

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="border-app-line flex items-center gap-2 border-b px-3 py-2">
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
			<div
				className={clsx(
					'min-h-0 flex-1 overflow-auto p-3',
					props.layout === 'grid'
						? 'grid gap-3'
						: 'flex flex-col gap-2'
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
			>
				{props.presentation.map((item) => (
					<button
						key={item.file.id}
						data-file-id={item.file.id}
						onClick={() => props.onSelectFile(item.file)}
						onDoubleClick={() => {
							if (item.file.kind === 'Directory' && props.onNavigateToDirectory) {
								props.onNavigateToDirectory(item.file);
							}
						}}
						className={clsx(
							'border-app-line bg-app-box/60 relative flex flex-col items-center justify-center rounded-xl border p-2',
							item.dimmed && 'opacity-50',
							item.file.id === props.selectedFileId &&
								'ring-accent ring-2'
						)}
					>
						<FileComponent.Thumb
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
				))}
			</div>
		</div>
	);
}
