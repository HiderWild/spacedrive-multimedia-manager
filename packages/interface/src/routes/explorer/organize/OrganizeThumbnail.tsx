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
 */
export function OrganizeThumbnail({file, size, className}: OrganizeThumbnailProps) {
	const {thumbnailData, isLoading, error} = useOrganizeThumbnail(file, size);

	if (error || !thumbnailData) {
		// Fallback to default thumb on error or while loading
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
