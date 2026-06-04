import { useState } from 'react';
import { Input, Label, dialogManager, useDialog, Dialog } from '@spacedrive/primitives';
import { useLibraryMutation } from '@sd/ts-client';
import { useForm } from 'react-hook-form';
import { useTranslation } from 'react-i18next';
import type { GroupType } from '@sd/ts-client';
import { i18n } from '../../i18n';

interface FormData {
	groupName: string;
}

export function useAddGroupDialog(spaceId: string) {
	return dialogManager.create((props) => <AddGroupDialog {...props} spaceId={spaceId} />);
}

function AddGroupDialog(props: { id: number; spaceId: string }) {
	const dialog = useDialog(props);
	const { t } = useTranslation('sidebar');
	const { t: tc } = useTranslation('common');
	const [groupType, setGroupType] = useState<GroupType>('Custom');

	const form = useForm<FormData>({
		defaultValues: { groupName: '' },
	});

	const addGroup = useLibraryMutation('spaces.add_group');

	const onSubmit = form.handleSubmit(async (data) => {
		await addGroup.mutateAsync({
			space_id: props.spaceId,
			name: data.groupName || getDefaultName(groupType),
			group_type: groupType,
		});
		form.reset();
		setGroupType('Custom');
		dialog.state.open = false;
	});

	return (
		<Dialog form={form} dialog={dialog} title={t('customize.addGroup')} onSubmit={onSubmit} ctaLabel={tc('create')}>
			<div className="space-y-4">
				<div>
					<Label>{t('customize.groupType')}</Label>
					<select
						value={typeof groupType === 'string' ? groupType : 'Custom'}
						onChange={(e) => setGroupType(e.target.value as GroupType)}
						className="w-full rounded-lg border border-app-line bg-app-input px-3 py-2 text-sm text-ink"
					>
						<option value="Devices">{t('customize.allDevices')}</option>
						<option value="Locations">{t('customize.allLocations')}</option>
						<option value="Tags">{t('sections.tags')}</option>
						<option value="Cloud">{t('customize.cloudStorage')}</option>
						<option value="Custom">{t('customize.custom')}</option>
					</select>
				</div>

				{groupType === 'Custom' && (
					<div>
						<Label>{t('customize.groupName')}</Label>
						<Input
							{...form.register('groupName')}
							placeholder={t('customize.groupNamePlaceholder')}
						/>
					</div>
				)}
			</div>
		</Dialog>
	);
}

function getDefaultName(groupType: GroupType): string {
	const t = (key: string) => i18n.t(key, { ns: 'sidebar' });
	if (groupType === 'Devices') return t('customize.allDevices');
	if (groupType === 'Locations') return t('sections.locations');
	if (groupType === 'Tags') return t('sections.tags');
	if (groupType === 'Cloud') return t('customize.cloudStorage');
	if (groupType === 'Custom') return t('customize.customGroup');
	if (typeof groupType === 'object' && 'Device' in groupType) return t('customize.device');
	return t('customize.group');
}

