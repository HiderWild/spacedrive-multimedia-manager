import type {File} from '@sd/ts-client';

export function redactFilename(value: string | null | undefined) {
	if (!value) {
		return value ?? null;
	}

	const extensionIndex = value.lastIndexOf('.');
	if (extensionIndex <= 0) {
		return '[redacted]';
	}

	return `[redacted]${value.slice(extensionIndex)}`;
}

export function redactPath(value: string | null | undefined) {
	if (!value) {
		return value ?? null;
	}

	const lastSeparatorIndex = Math.max(
		value.lastIndexOf('/'),
		value.lastIndexOf('\\')
	);

	if (lastSeparatorIndex === -1) {
		return redactFilename(value);
	}

	const prefix = value.slice(0, lastSeparatorIndex + 1);
	const leaf = value.slice(lastSeparatorIndex + 1);
	return `${prefix}${redactFilename(leaf)}`;
}

export function redactUrl(value: string | null | undefined) {
	if (!value) {
		return value ?? null;
	}

	const queryIndex = value.search(/[?#]/);
	const base = queryIndex === -1 ? value : value.slice(0, queryIndex);
	const suffix = queryIndex === -1 ? '' : value.slice(queryIndex);

	if (!base.includes('/') && !base.includes('\\')) {
		return '[redacted]';
	}

	return `${redactPath(base)}${suffix}`;
}

export function sanitizeSdPath(sdPath: File['sd_path'] | null | undefined) {
	if (!sdPath) {
		return null;
	}

	if ('Physical' in sdPath) {
		return {
			type: 'Physical',
			path: redactPath(sdPath.Physical.path),
			deviceSlug: sdPath.Physical.device_slug
		};
	}

	if ('Cloud' in sdPath) {
		return {
			type: 'Cloud',
			path: redactPath(sdPath.Cloud.path),
			service: sdPath.Cloud.service,
			identifier: sdPath.Cloud.identifier
		};
	}

	return {type: 'Unknown'};
}

export function formatDebugError(error: unknown) {
	if (!error) {
		return null;
	}

	if (error instanceof Error) {
		return `${error.name}: ${error.message}`;
	}

	return String(error);
}
