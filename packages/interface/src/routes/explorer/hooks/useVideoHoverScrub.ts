import { useCallback, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { File, Sidecar } from "@sd/ts-client";
import { getContentKind } from "@sd/ts-client";
import { useServer } from "../../../contexts/ServerContext";

/**
 * Grid dimensions for a thumbstrip sprite sheet.
 *
 * The backend stores thumbstrips as a single WebP sprite where frames are laid
 * out in a `columns × rows` grid. The grid size is NOT persisted on the sidecar
 * row (the `Sidecar` type only carries kind/variant/format), so it is derived
 * from the variant name, which maps 1:1 to the configs in
 * `core/src/ops/media/thumbstrip/config.rs`.
 */
interface ThumbstripGrid {
	columns: number;
	rows: number;
}

/**
 * Maps a thumbstrip variant name to its grid dimensions.
 *
 * Mirrors `ThumbstripVariants` in core: `thumbstrip_preview` (5×5),
 * `thumbstrip_detailed` (10×10), `thumbstrip_mobile` (3×3). Falls back to the
 * 5×5 preview layout for unknown variants since that is the default the backend
 * auto-generates.
 */
function gridForVariant(variant: string): ThumbstripGrid {
	if (variant.includes("detailed")) return { columns: 10, rows: 10 };
	if (variant.includes("mobile")) return { columns: 3, rows: 3 };
	return { columns: 5, rows: 5 };
}

export interface VideoHoverScrub {
	/** True when the file is a video that has a usable thumbstrip sidecar. */
	enabled: boolean;
	/** True while the pointer is hovering and a frame should be shown. */
	isScrubbing: boolean;
	/** Cursor position along the tile, 0..1, used for the progress indicator. */
	progress: number;
	/** Ref to attach to the interactive container that captures pointer X. */
	containerRef: React.RefObject<HTMLDivElement>;
	/** Inline style for the sprite layer (background image/size/position). */
	spriteStyle: CSSProperties | null;
	/** Pointer handlers driving the scrub. */
	onPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
	onPointerEnter: () => void;
	onPointerLeave: () => void;
}

/**
 * Drives an in-grid video hover-scrub over an existing thumbstrip sidecar.
 *
 * As the pointer moves across a video tile, the frame shown follows the cursor's
 * X position. The frame index is computed with the Video-Hub-App filmstrip
 * formula `floor(cursorXRatio * frameCount)` (clamped to the last frame), and
 * the corresponding sprite cell is exposed through CSS `background-position`.
 *
 * When the file is not a video or has no thumbstrip sidecar, `enabled` is false
 * and the caller should render the static thumbnail unchanged (no layout shift,
 * no behavior change).
 */
export function useVideoHoverScrub(
	file: File,
	size: number,
	squareMode: boolean,
): VideoHoverScrub {
	const { buildSidecarUrl } = useServer();
	const containerRef = useRef<HTMLDivElement>(null);
	const [progress, setProgress] = useState(0);
	const [isScrubbing, setIsScrubbing] = useState(false);

	const thumbstrip: Sidecar | undefined = file.sidecars?.find(
		(s) => s.kind === "thumbstrip",
	);

	const isVideo = getContentKind(file) === "video";
	const contentUuid = file.content_identity?.uuid ?? null;

	const thumbstripUrl =
		thumbstrip && contentUuid
			? buildSidecarUrl(
					contentUuid,
					thumbstrip.kind,
					thumbstrip.variant,
					thumbstrip.format,
				)
			: null;

	const enabled = isVideo && thumbstrip !== undefined && thumbstripUrl !== null;

	const onPointerMove = useCallback(
		(e: React.PointerEvent<HTMLDivElement>) => {
			const el = containerRef.current;
			if (!el) return;
			const rect = el.getBoundingClientRect();
			if (rect.width === 0) return;
			const ratio = (e.clientX - rect.left) / rect.width;
			setProgress(Math.max(0, Math.min(1, ratio)));
		},
		[],
	);

	const onPointerEnter = useCallback(() => setIsScrubbing(true), []);
	const onPointerLeave = useCallback(() => {
		setIsScrubbing(false);
		setProgress(0);
	}, []);

	const spriteStyle = useMemo<CSSProperties | null>(() => {
		if (!enabled || !thumbstrip || !thumbstripUrl) return null;

		const grid = gridForVariant(thumbstrip.variant);
		const totalFrames = grid.columns * grid.rows;

		// Filmstrip offset formula (Video-Hub-App): the frame index follows the
		// cursor's X ratio across the tile, clamped to the final frame.
		const frameIndex = Math.min(
			Math.floor(progress * totalFrames),
			totalFrames - 1,
		);
		const col = frameIndex % grid.columns;
		const row = Math.floor(frameIndex / grid.columns);

		// Each cell is sized as a percentage of the container. background-size is
		// the full grid expressed as a percentage of one cell (columns*100%).
		const aspect =
			file.video_media_data?.width && file.video_media_data?.height
				? file.video_media_data.width / file.video_media_data.height
				: 16 / 9;

		let bgWidth = grid.columns * 100;
		let bgHeight = grid.rows * 100;

		if (squareMode) {
			// Cover the square tile: scale the axis that would otherwise letterbox.
			if (aspect > 1) {
				bgWidth = grid.columns * 100 * aspect;
			} else {
				bgHeight = (grid.rows * 100) / aspect;
			}
		}

		// CSS percentage background-position: position = (col/cols)*bgSize scaled
		// into the (bgSize - 100) travel range so cells align exactly.
		const spriteX =
			grid.columns > 1
				? ((col / grid.columns) * bgWidth) / (bgWidth - 100) * 100
				: 0;
		const spriteY =
			grid.rows > 1
				? ((row / grid.rows) * bgHeight) / (bgHeight - 100) * 100
				: 0;

		return {
			backgroundImage: `url(${thumbstripUrl})`,
			backgroundSize: `${bgWidth}% ${bgHeight}%`,
			backgroundPosition: `${spriteX}% ${spriteY}%`,
			backgroundRepeat: "no-repeat",
			imageRendering: "crisp-edges",
		};
	}, [enabled, thumbstrip, thumbstripUrl, progress, squareMode, file.video_media_data, size]);

	return {
		enabled,
		isScrubbing,
		progress,
		containerRef,
		spriteStyle,
		onPointerMove,
		onPointerEnter,
		onPointerLeave,
	};
}
