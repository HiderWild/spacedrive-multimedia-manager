import { useMemo } from "react";
import justifiedLayout from "justified-layout";
import type { File } from "@sd/ts-client";

/**
 * Default aspect ratio (1:1) for files whose real dimensions are unknown.
 *
 * Per-file width/height only exist for files that have had media metadata
 * extracted (`image_media_data` / `video_media_data`). Anything else, including
 * directories and not-yet-indexed media, falls back to a square so the row
 * solver still has a usable ratio. Real dimensions are a backend follow-up.
 */
const FALLBACK_ASPECT = 1;

/** Clamp extreme ratios so a single panorama can't dominate a whole row. */
const MIN_ASPECT = 0.3;
const MAX_ASPECT = 3.0;

export interface JustifiedBox {
	file: File;
	index: number;
	top: number;
	left: number;
	width: number;
	height: number;
}

export interface JustifiedLayoutResult {
	boxes: JustifiedBox[];
	containerHeight: number;
}

export interface UseJustifiedLayoutOptions {
	targetRowHeight: number;
	boxSpacing: number;
	containerPadding: number;
}

/**
 * Reads a file's intrinsic aspect ratio from its extracted media metadata.
 *
 * Image and video metadata both carry pixel `width`/`height`. When neither is
 * present (directories, audio, un-indexed files) we return the square fallback
 * so the layout stays stable instead of collapsing the row.
 */
function getAspectRatio(file: File): number {
	const media = file.image_media_data ?? file.video_media_data;
	if (media && media.width > 0 && media.height > 0) {
		const ratio = media.width / media.height;
		return Math.min(MAX_ASPECT, Math.max(MIN_ASPECT, ratio));
	}
	return FALLBACK_ASPECT;
}

/**
 * Computes a Flickr-style justified-rows layout for the given files.
 *
 * The heavy work (running the row solver over every aspect ratio) is memoized
 * on the inputs that actually change the geometry: the container width, the
 * per-file aspect ratios, and the spacing options. File identity is folded into
 * a lightweight signature so re-renders that don't change the set or its
 * dimensions reuse the previous result.
 */
export function useJustifiedLayout(
	files: File[],
	containerWidth: number,
	options: UseJustifiedLayoutOptions,
): JustifiedLayoutResult {
	const { targetRowHeight, boxSpacing, containerPadding } = options;

	const aspectRatios = useMemo(() => files.map(getAspectRatio), [files]);

	// Signature changes only when the geometry-relevant inputs change, avoiding
	// a full re-solve when unrelated file fields update.
	const aspectSignature = useMemo(
		() => aspectRatios.map((r) => r.toFixed(3)).join(","),
		[aspectRatios],
	);

	return useMemo<JustifiedLayoutResult>(() => {
		if (containerWidth <= 0 || files.length === 0) {
			return { boxes: [], containerHeight: 0 };
		}

		const result = justifiedLayout(aspectRatios, {
			containerWidth,
			containerPadding,
			boxSpacing,
			targetRowHeight,
			showWidows: true,
		});

		const boxes: JustifiedBox[] = result.boxes.map((box, index) => ({
			file: files[index],
			index,
			top: box.top,
			left: box.left,
			width: box.width,
			height: box.height,
		}));

		return { boxes, containerHeight: result.containerHeight };
		// aspectSignature stands in for aspectRatios identity to keep the solve
		// stable across renders that don't change geometry.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [
		files,
		containerWidth,
		targetRowHeight,
		boxSpacing,
		containerPadding,
		aspectSignature,
	]);
}
