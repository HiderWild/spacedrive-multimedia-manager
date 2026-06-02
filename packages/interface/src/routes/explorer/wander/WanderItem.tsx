import { memo, useEffect, useRef, useState } from "react";
import type { File } from "@sd/ts-client";
import { getContentKind } from "@sd/ts-client";
import { Thumb } from "../File/Thumb";

interface WanderItemProps {
	file: File;
	/** Resolved media URL for this file, from the shared resolver. */
	url: string | null;
	/** Mirrors the stream's play state so video pauses with the slideshow. */
	isPlaying: boolean;
	/** Fired when a video finishes so the engine can advance. */
	onVideoEnded: () => void;
}

/**
 * Renders a single wander item full-bleed.
 *
 * Images render as a contained `<img>`; videos autoplay muted and report
 * `ended` so the engine advances when they finish instead of on a timer. Both
 * fall back to the existing `Thumb` (which resolves sidecar thumbnails) when no
 * playable URL is available or the original fails to load, so the slideshow
 * never shows a blank frame.
 */
export const WanderItem = memo(function WanderItem({
	file,
	url,
	isPlaying,
	onVideoEnded,
}: WanderItemProps) {
	const kind = getContentKind(file);
	const isVideo = kind === "video";
	const isImage = kind === "image";
	const [errored, setErrored] = useState(false);
	const videoRef = useRef<HTMLVideoElement>(null);

	// Keep video playback in sync with the slideshow's play/pause state.
	useEffect(() => {
		const video = videoRef.current;
		if (!video) return;
		if (isPlaying) {
			void video.play().catch(() => {});
		} else {
			video.pause();
		}
	}, [isPlaying, url]);

	const showFallback = !url || errored || (!isVideo && !isImage);

	if (showFallback) {
		return (
			<div className="flex h-full w-full items-center justify-center">
				<Thumb file={file} size={800} className="max-h-full max-w-full" />
			</div>
		);
	}

	if (isVideo) {
		return (
			<video
				ref={videoRef}
				key={url}
				src={url ?? undefined}
				className="max-h-full max-w-full object-contain"
				autoPlay={isPlaying}
				muted
				playsInline
				controls={false}
				onEnded={onVideoEnded}
				onError={() => setErrored(true)}
			/>
		);
	}

	return (
		<img
			key={url}
			src={url ?? undefined}
			alt={file.name}
			className="max-h-full max-w-full object-contain"
			onError={() => setErrored(true)}
		/>
	);
});
