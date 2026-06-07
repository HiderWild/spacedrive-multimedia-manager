export interface HelpShortcutItem {
	keys: string;
	descriptionKey:
		| 'help.items.resizeInspector'
		| 'help.items.imagePrevious'
		| 'help.items.imageNext'
		| 'help.items.imageZoom'
		| 'help.items.imageOpenWindow'
		| 'help.items.videoPrevious'
		| 'help.items.videoNext'
		| 'help.items.videoSeek'
		| 'help.items.videoFrameStep'
		| 'help.items.videoToggle';
}

export interface HelpShortcutSection {
	id: 'layout' | 'image' | 'video';
	titleKey:
		| 'help.sections.layout'
		| 'help.sections.image'
		| 'help.sections.video';
	items: HelpShortcutItem[];
}

export const organizeHelpSections: HelpShortcutSection[] = [
	{
		id: 'layout',
		titleKey: 'help.sections.layout',
		items: [
			{
				keys: 'Drag divider',
				descriptionKey: 'help.items.resizeInspector'
			}
		]
	},
	{
		id: 'image',
		titleKey: 'help.sections.image',
		items: [
			{
				keys: 'Up / Left',
				descriptionKey: 'help.items.imagePrevious'
			},
			{
				keys: 'Down / Right',
				descriptionKey: 'help.items.imageNext'
			},
			{
				keys: 'Wheel',
				descriptionKey: 'help.items.imageZoom'
			},
			{
				keys: 'Space',
				descriptionKey: 'help.items.imageOpenWindow'
			}
		]
	},
	{
		id: 'video',
		titleKey: 'help.sections.video',
		items: [
			{
				keys: 'Up / Down',
				descriptionKey: 'help.items.videoPrevious'
			},
			{
				keys: 'Left / Right',
				descriptionKey: 'help.items.videoSeek'
			},
			{
				keys: 'Wheel',
				descriptionKey: 'help.items.videoFrameStep'
			},
			{
				keys: 'Click / Space',
				descriptionKey: 'help.items.videoToggle'
			}
		]
	}
];
