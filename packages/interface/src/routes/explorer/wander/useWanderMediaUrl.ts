import { useCallback } from "react";
import type { File, Sidecar } from "@sd/ts-client";
import { usePlatform } from "../../../contexts/PlatformContext";
import { useServer } from "../../../contexts/ServerContext";

/**
 * Picks the highest-resolution thumbnail sidecar for a file.
 *
 * Variants are named like `grid@1x` / `detail@2x`; the leading number is the
 * pixel size. Returns the largest so the slideshow has the sharpest available
 * fallback when an original cannot be loaded.
 */
function highestResThumb(file: File): Sidecar | null {
	const thumbnails = file.sidecars.filter((s) => s.kind === "thumb");
	if (thumbnails.length === 0) return null;
	return [...thumbnails].sort((a, b) => {
		const aSize = parseInt(a.variant.split("x")[0]?.replace(/\D/g, "") || "0");
		const bSize = parseInt(b.variant.split("x")[0]?.replace(/\D/g, "") || "0");
		return bSize - aSize;
	})[0];
}

/**
 * Resolves a displayable media URL for a file, mirroring the explorer's
 * existing renderers.
 *
 * Originals are served through `platform.convertFileSrc` over the file's
 * physical path, exactly as `ContentRenderer` does for images and videos, so
 * no new URL scheme is invented. When the file has no physical path (cloud or
 * content-addressed) or the platform cannot convert it, the highest-resolution
 * thumbnail sidecar is used via `buildSidecarUrl`. Returns null when nothing is
 * resolvable so callers can skip preloading.
 */
export function useWanderMediaUrl(): (file: File) => string | null {
	const platform = usePlatform();
	const { buildSidecarUrl } = useServer();

	return useCallback(
		(file: File): string | null => {
			const path = file.sd_path;
			if (platform.convertFileSrc && "Physical" in path) {
				return platform.convertFileSrc(path.Physical.path);
			}

			const uuid = file.content_identity?.uuid;
			const thumb = highestResThumb(file);
			if (uuid && thumb) {
				return buildSidecarUrl(uuid, thumb.kind, thumb.variant, thumb.format);
			}

			return null;
		},
		[platform, buildSidecarUrl],
	);
}
