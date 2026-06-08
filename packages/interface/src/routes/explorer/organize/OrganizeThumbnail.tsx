import {useOrganizeThumbnail} from './useOrganizeThumbnail';
import {File as FileComponent} from '../File';
import type {File} from '@sd/ts-client';

interface OrganizeThumbnailProps {
	file: File;
	size: number;
	className?: string;
}

/**
 * Cached thumbnail component for organize view
 * Uses in-memory cache with FIFO eviction and concurrent load limiting
 * Falls back to default FileComponent.Thumb if cache system fails
 */
export function OrganizeThumbnail({file, size, className}: OrganizeThumbnailProps) {
	// Check if file has required data for custom thumbnail loading
	const hasContentIdentity = !!file.content_identity?.uuid;
	const hasSidecars = file.sidecars && file.sidecars.length > 0;

	// Only use custom thumbnail system if file has the required metadata
	const shouldUseCache = hasContentIdentity && hasSidecars;

	const {thumbnailData, isLoading, error} = useOrganizeThumbnail(
		file,
		size
	);

	// Always fall back to default thumb if:
	// - Cache system not applicable for this file
	// - Error loading thumbnail
	// - No thumbnail data yet
	if (!shouldUseCache || error || !thumbnailData) {
		return <FileComponent.Thumb file={file} size={size} className={className} />;
	}

	return (
		<img
			src={thumbnailData}
			alt={file.name}
			className={className}
			style={{
				width: size,
				height: size,
				objectFit: 'cover'
			}}
		/>
	);
}
