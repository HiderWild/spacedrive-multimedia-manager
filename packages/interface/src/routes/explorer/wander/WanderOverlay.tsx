import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import {
	CaretLeft,
	CaretRight,
	Pause,
	Play,
	Shuffle,
	X,
} from "@phosphor-icons/react";
import clsx from "clsx";
import type { File } from "@sd/ts-client";
import { useWanderStream } from "../hooks/useWanderStream";
import { useWanderMediaUrl } from "./useWanderMediaUrl";
import { WanderItem } from "./WanderItem";
import { QuickManageOverlay } from "./quickmanage/QuickManageOverlay";

type WanderPaneCount = 1 | 2 | 4;

export interface WanderOverlayProps {
	/** The media set to wander through (the explorer's current files). */
	files: File[];
	/** Index into `files` to start from (current selection or first item). */
	startIndex?: number;
	/** Whether more files can be paged in from the active source. */
	hasNextPage: boolean;
	/** Pull the next page from the underlying infinite query. */
	fetchNextPage: () => void;
	/** Close the overlay. */
	onClose: () => void;
}

/**
 * Full-screen multi-pane immersive slideshow overlay ("wander" mode).
 *
 * Mounted in a portal on top of the explorer. Each pane owns an independent
 * `useWanderStream`, so panes can be paused, advanced, and shuffled without
 * affecting the others. Keyboard controls target the first pane; pane-local
 * buttons handle the rest.
 */
export function WanderOverlay({
	files,
	startIndex = 0,
	hasNextPage,
	fetchNextPage,
	onClose,
}: WanderOverlayProps) {
	const [paneCount, setPaneCount] = useState<WanderPaneCount>(1);
	const [primaryControls, setPrimaryControls] = useState<{
		next: () => void;
		prev: () => void;
		togglePlay: () => void;
	} | null>(null);

	const handleKeyDown = useCallback(
		(e: KeyboardEvent) => {
			switch (e.key) {
				case "Escape":
					e.preventDefault();
					onClose();
					break;
				case " ":
					e.preventDefault();
					primaryControls?.togglePlay();
					break;
				case "ArrowLeft":
					e.preventDefault();
					primaryControls?.prev();
					break;
				case "ArrowRight":
					e.preventDefault();
					primaryControls?.next();
					break;
				default:
					break;
			}
		},
		[onClose, primaryControls],
	);

	useEffect(() => {
		window.addEventListener("keydown", handleKeyDown);
		return () => window.removeEventListener("keydown", handleKeyDown);
	}, [handleKeyDown]);

	const paneStarts = useMemo(
		() => Array.from({ length: paneCount }, (_, i) => startIndex + i),
		[paneCount, startIndex],
	);

	const overlay = (
		<div className="fixed inset-0 z-[9999] flex flex-col bg-black">
			<div
				className={clsx(
					"grid min-h-0 flex-1 gap-px bg-white/10",
					paneCount === 1 && "grid-cols-1",
					paneCount === 2 && "grid-cols-2",
					paneCount === 4 && "grid-cols-2 grid-rows-2",
				)}
			>
				{paneStarts.map((paneStart, index) => (
					<WanderPane
						key={`${paneCount}-${index}`}
						files={files}
						startIndex={paneStart}
						hasNextPage={hasNextPage}
						fetchNextPage={fetchNextPage}
						isPrimary={index === 0}
						onPrimaryControls={setPrimaryControls}
					/>
				))}
			</div>

			{/* Close button */}
			<button
				type="button"
				aria-label="Close"
				onClick={onClose}
				className="absolute right-4 top-4 rounded-full bg-black/40 p-2 text-white/80 transition hover:bg-black/60 hover:text-white"
			>
				<X size={20} weight="bold" />
			</button>

			{/* Bottom control bar */}
			<div className="flex items-center justify-center gap-2 bg-black/70 px-4 py-3">
				<div className="mr-2 text-xs font-medium text-white/60">Panes</div>
				{([1, 2, 4] as const).map((count) => (
					<button
						key={count}
						type="button"
						onClick={() => setPaneCount(count)}
						className={clsx(
							"flex h-8 min-w-8 items-center justify-center rounded-md px-2 text-xs font-medium transition",
							paneCount === count
								? "bg-accent text-white"
								: "bg-white/10 text-white/70 hover:bg-white/20 hover:text-white",
						)}
					>
						{count}
					</button>
				))}
				<div className="ml-3 text-xs text-white/40">
					Space and arrows control pane 1
				</div>
			</div>
		</div>
	);

	return createPortal(overlay, document.body);
}

interface WanderPaneProps {
	files: File[];
	startIndex: number;
	hasNextPage: boolean;
	fetchNextPage: () => void;
	isPrimary: boolean;
	onPrimaryControls: (
		controls: {
			next: () => void;
			prev: () => void;
			togglePlay: () => void;
		} | null,
	) => void;
}

function WanderPane({
	files,
	startIndex,
	hasNextPage,
	fetchNextPage,
	isPrimary,
	onPrimaryControls,
}: WanderPaneProps) {
	const resolveMediaUrl = useWanderMediaUrl();

	const stream = useWanderStream({
		files,
		startIndex,
		enabled: true,
		hasNextPage,
		fetchNextPage,
		resolveMediaUrl,
	});

	const currentUrl = useMemo(
		() => (stream.current ? resolveMediaUrl(stream.current) : null),
		[stream.current, resolveMediaUrl],
	);

	useEffect(() => {
		if (!isPrimary) return;
		onPrimaryControls({
			next: stream.next,
			prev: stream.prev,
			togglePlay: stream.togglePlay,
		});
		return () => onPrimaryControls(null);
	}, [isPrimary, onPrimaryControls, stream.next, stream.prev, stream.togglePlay]);

	return (
		<div className="group relative min-h-0 overflow-hidden bg-black">
			{stream.current ? (
				<WanderItem
					file={stream.current}
					url={currentUrl}
					isPlaying={stream.isPlaying}
					onVideoEnded={stream.onVideoEnded}
				/>
			) : (
				<div className="flex h-full items-center justify-center text-ink-faint text-sm">
					No media to wander through.
				</div>
			)}

			<QuickManageOverlay file={stream.current} onAdvance={stream.next} />

			<button
				type="button"
				aria-label="Previous"
				onClick={stream.prev}
				className="absolute inset-y-0 left-0 w-1/5 cursor-w-resize focus:outline-none"
			/>
			<button
				type="button"
				aria-label="Next"
				onClick={stream.next}
				className="absolute inset-y-0 right-0 w-1/5 cursor-e-resize focus:outline-none"
			/>

			<div className="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-2 rounded-full bg-black/55 px-2 py-1.5 opacity-0 backdrop-blur transition group-hover:opacity-100 group-focus-within:opacity-100">
				<ControlButton label="Previous" onClick={stream.prev} compact>
					<CaretLeft size={18} weight="bold" />
				</ControlButton>
				<ControlButton
					label={stream.isPlaying ? "Pause" : "Play"}
					onClick={stream.togglePlay}
					compact
				>
					{stream.isPlaying ? (
						<Pause size={18} weight="fill" />
					) : (
						<Play size={18} weight="fill" />
					)}
				</ControlButton>
				<ControlButton label="Next" onClick={stream.next} compact>
					<CaretRight size={18} weight="bold" />
				</ControlButton>
				<ControlButton
					label="Shuffle"
					onClick={stream.toggleShuffle}
					active={stream.shuffle}
					compact
				>
					<Shuffle size={18} weight="bold" />
				</ControlButton>
				<div className="min-w-[56px] text-center text-[11px] tabular-nums text-white/60">
					{stream.total > 0 ? stream.position + 1 : 0} / {stream.total}
				</div>
			</div>

			{isPrimary && (
				<div className="absolute left-3 top-3 rounded-full bg-accent px-2 py-0.5 text-[11px] font-semibold text-white">
					Pane 1
				</div>
			)}
		</div>
	);
}

interface ControlButtonProps {
	label: string;
	onClick: () => void;
	active?: boolean;
	compact?: boolean;
	children: React.ReactNode;
}

function ControlButton({
	label,
	onClick,
	active,
	compact,
	children,
}: ControlButtonProps) {
	return (
		<button
			type="button"
			aria-label={label}
			onClick={onClick}
			className={clsx(
				"flex items-center justify-center rounded-full transition",
				compact ? "h-8 w-8" : "h-10 w-10",
				active
					? "bg-accent text-white"
					: "bg-white/10 text-white/80 hover:bg-white/20 hover:text-white",
			)}
		>
			{children}
		</button>
	);
}
