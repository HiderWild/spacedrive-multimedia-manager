import {describe, expect, test} from 'bun:test';
import {organizeHelpSections} from '../helpSettingsContent';

describe('organizeHelpSections', () => {
	test('documents image preview shortcuts', () => {
		const imageSection = organizeHelpSections.find(
			(section) => section.id === 'image'
		);

		expect(imageSection?.items.map((item) => item.keys)).toContain(
			'Up / Left'
		);
		expect(imageSection?.items.map((item) => item.keys)).toContain(
			'Down / Right'
		);
		expect(imageSection?.items.map((item) => item.keys)).toContain('Wheel');
		expect(imageSection?.items.map((item) => item.keys)).toContain('Space');
	});

	test('documents video preview shortcuts', () => {
		const videoSection = organizeHelpSections.find(
			(section) => section.id === 'video'
		);

		expect(videoSection?.items.map((item) => item.keys)).toContain(
			'Up / Down'
		);
		expect(videoSection?.items.map((item) => item.keys)).toContain(
			'Left / Right'
		);
		expect(videoSection?.items.map((item) => item.keys)).toContain('Wheel');
		expect(videoSection?.items.map((item) => item.keys)).toContain(
			'Click / Space'
		);
	});
});
