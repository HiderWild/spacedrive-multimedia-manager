import {describe, expect, test} from 'bun:test';
import {
	buildExtractTextInput,
	buildGenerateProxyInput,
	buildGenerateThumbstripInput,
	buildRegenerateThumbnailInput,
	buildTranscribeAudioInput
} from '../mediaActionInputs';

describe('mediaActionInputs', () => {
	test('buildRegenerateThumbnailInput uses the default thumbnail variants', () => {
		expect(buildRegenerateThumbnailInput('entry-1', true)).toEqual({
			entry_uuid: 'entry-1',
			variants: ['grid@1x', 'grid@2x', 'detail@1x'],
			force: true
		});
	});

	test('buildExtractTextInput defaults OCR to English', () => {
		expect(buildExtractTextInput('entry-2', false)).toEqual({
			entry_uuid: 'entry-2',
			languages: ['eng'],
			force: false
		});
	});

	test('buildGenerateThumbstripInput requests both preview variants', () => {
		expect(buildGenerateThumbstripInput('entry-3', false)).toEqual({
			entry_uuid: 'entry-3',
			variants: ['thumbstrip_preview', 'thumbstrip_detailed'],
			force: false
		});
	});

	test('buildTranscribeAudioInput uses the backend default model contract', () => {
		expect(buildTranscribeAudioInput('entry-4')).toEqual({
			entry_uuid: 'entry-4',
			model: 'base',
			language: null
		});
	});

	test('buildGenerateProxyInput uses the scrubbing preset with hardware accel', () => {
		expect(buildGenerateProxyInput('entry-5', false)).toEqual({
			entry_uuid: 'entry-5',
			resolution: 'scrubbing',
			force: false,
			use_hardware_accel: true
		});
	});
});
