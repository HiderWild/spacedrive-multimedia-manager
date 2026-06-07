import {describe, expect, test} from 'bun:test';
import {
	redactFilename,
	redactPath,
	redactUrl
} from '../organizeDebug';

describe('organize debug redaction', () => {
	test('redactFilename removes the visible base name but preserves extension', () => {
		expect(redactFilename('photo.final.JPG')).toBe('[redacted].JPG');
		expect(redactFilename('README')).toBe('[redacted]');
	});

	test('redactPath removes the trailing file or directory name', () => {
		expect(redactPath('/Users/demo/Pictures/photo.jpg')).toBe(
			'/Users/demo/Pictures/[redacted].jpg'
		);
		expect(redactPath('C:\\Media\\Trips\\Summer')).toBe(
			'C:\\Media\\Trips\\[redacted]'
		);
	});

	test('redactUrl removes the trailing visible file name', () => {
		expect(
			redactUrl('asset://localhost/C:/Media/Trips/photo.jpg')
		).toBe('asset://localhost/C:/Media/Trips/[redacted].jpg');
		expect(redactUrl('not-a-url')).toBe('[redacted]');
	});
});
