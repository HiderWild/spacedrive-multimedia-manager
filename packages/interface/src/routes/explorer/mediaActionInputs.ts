import type {
	ExtractTextInput,
	GenerateProxyInput,
	GenerateThumbstripInput,
	RegenerateThumbnailInput,
	TranscribeAudioInput
} from '@sd/ts-client';

const DEFAULT_THUMBNAIL_VARIANTS = ['grid@1x', 'grid@2x', 'detail@1x'] as const;
const DEFAULT_OCR_LANGUAGES = ['eng'] as const;
const DEFAULT_THUMBSTRIP_VARIANTS = [
	'thumbstrip_preview',
	'thumbstrip_detailed'
] as const;
const DEFAULT_TRANSCRIPTION_MODEL = 'base';
const DEFAULT_PROXY_RESOLUTION = 'scrubbing';

export function buildRegenerateThumbnailInput(
	entryUuid: string,
	force: boolean
): RegenerateThumbnailInput {
	return {
		entry_uuid: entryUuid,
		variants: [...DEFAULT_THUMBNAIL_VARIANTS],
		force
	};
}

export function buildExtractTextInput(
	entryUuid: string,
	force: boolean
): ExtractTextInput {
	return {
		entry_uuid: entryUuid,
		languages: [...DEFAULT_OCR_LANGUAGES],
		force
	};
}

export function buildGenerateThumbstripInput(
	entryUuid: string,
	force: boolean
): GenerateThumbstripInput {
	return {
		entry_uuid: entryUuid,
		variants: [...DEFAULT_THUMBSTRIP_VARIANTS],
		force
	};
}

export function buildTranscribeAudioInput(
	entryUuid: string
): TranscribeAudioInput {
	return {
		entry_uuid: entryUuid,
		model: DEFAULT_TRANSCRIPTION_MODEL,
		language: null
	};
}

export function buildGenerateProxyInput(
	entryUuid: string,
	force: boolean
): GenerateProxyInput {
	return {
		entry_uuid: entryUuid,
		resolution: DEFAULT_PROXY_RESOLUTION,
		force,
		use_hardware_accel: true
	};
}
