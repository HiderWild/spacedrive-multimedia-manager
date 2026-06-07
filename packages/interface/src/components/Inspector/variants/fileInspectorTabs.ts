import {
	ChatCircle,
	ClockCounterClockwise,
	DotsThree,
	Info,
	MapPin,
	Paperclip
} from '@phosphor-icons/react';

export interface FileInspectorTab {
	id: string;
	label: string;
	icon: typeof Info;
	badge?: number;
	disabled?: boolean;
	tooltip?: string;
}

export function buildFileInspectorTabs(args: {
	isDev: boolean;
	previewTabs: FileInspectorTab[];
}): FileInspectorTab[] {
	const baseTabs: FileInspectorTab[] = [
		{id: 'overview', label: 'Overview', icon: Info},
		{id: 'sidecars', label: 'Sidecars', icon: Paperclip},
		{id: 'instances', label: 'Instances', icon: MapPin},
		...(args.isDev
			? [{id: 'chat', label: 'Chat', icon: ChatCircle, badge: 3}]
			: []),
		...(args.isDev
			? [
					{
						id: 'activity',
						label: 'Activity',
						icon: ClockCounterClockwise
					}
				]
			: []),
		{id: 'details', label: 'More', icon: DotsThree}
	];

	return [...args.previewTabs, ...baseTabs];
}
