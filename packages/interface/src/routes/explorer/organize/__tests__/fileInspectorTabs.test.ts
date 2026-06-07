import {Image, Paperclip} from '@phosphor-icons/react';
import {describe, expect, test} from 'bun:test';
import {buildFileInspectorTabs} from '../../../../components/Inspector/variants/fileInspectorTabs';

describe('buildFileInspectorTabs', () => {
	test('uses a distinct icon for sidecars when organize preview injects an image tab', () => {
		const tabs = buildFileInspectorTabs({
			isDev: false,
			previewTabs: [
				{
					id: 'image',
					label: 'Preview image',
					icon: Image,
					disabled: false,
					tooltip: 'Preview image'
				}
			]
		});

		expect(tabs.map((tab) => tab.id)).toEqual([
			'image',
			'overview',
			'sidecars',
			'instances',
			'details'
		]);
		expect(tabs.find((tab) => tab.id === 'sidecars')?.icon).toBe(Paperclip);
		expect(tabs.find((tab) => tab.id === 'sidecars')?.icon).not.toBe(Image);
	});
});
