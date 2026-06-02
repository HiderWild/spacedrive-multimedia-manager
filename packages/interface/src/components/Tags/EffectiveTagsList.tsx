import { ArrowCounterClockwise, EyeSlash } from '@phosphor-icons/react';
import type { EffectiveTag } from '@sd/ts-client';
import { toast } from '@spacedrive/primitives';
import clsx from 'clsx';
import {
	useLibraryMutation,
	useLibraryQuery
} from '../../contexts/SpacedriveContext';
import { useRefetchTagQueries } from '../../hooks/useRefetchTagQueries';
import { Tag } from '../Inspector/primitives/Tag';

interface EffectiveTagsListProps {
	/** Entry UUID (File.id) whose effective (direct + inherited) tags are shown. */
	fileId: string;
}

/**
 * Renders an entry's effective tags with inheritance provenance.
 *
 * Effective tags come from the `tags.effective` query (task A-02), which labels
 * each tag's origin so the UI can distinguish a directly applied tag from one
 * inherited via an ancestor folder, or one that has been explicitly suppressed.
 * Direct tags can be removed, inherited tags can be overridden (suppressed), and
 * overridden tags can have that override cleared so inheritance resumes.
 */
export function EffectiveTagsList({ fileId }: EffectiveTagsListProps) {
	const refetchTagQueries = useRefetchTagQueries();

	const effectiveQuery = useLibraryQuery(
		{ type: 'tags.effective', input: { entry_id: fileId } },
		{ enabled: !!fileId }
	);

	const overrideTag = useLibraryMutation('tags.override', {
		onSuccess: refetchTagQueries
	});
	const removeOverride = useLibraryMutation('tags.remove_override', {
		onSuccess: refetchTagQueries
	});
	const unapplyTag = useLibraryMutation('tags.unapply', {
		onSuccess: refetchTagQueries
	});

	const tags = effectiveQuery.data?.tags ?? [];

	if (tags.length === 0) return null;

	return (
		<>
			{tags.map((et) => (
				<EffectiveTagChip
					key={et.tag.id}
					tag={et}
					onRemoveDirect={async () => {
						try {
							await unapplyTag.mutateAsync({
								entry_ids: [fileId],
								tag_ids: [et.tag.id]
							});
						} catch (err) {
							console.error('Failed to remove tag:', err);
							toast.error(`Failed to remove tag: ${err}`);
						}
					}}
					onOverride={async () => {
						try {
							await overrideTag.mutateAsync({
								entry_id: fileId,
								tag_id: et.tag.id,
								source_ancestor_id: et.source_entry_id
							});
						} catch (err) {
							console.error('Failed to override tag:', err);
							toast.error(`Failed to override tag: ${err}`);
						}
					}}
					onClearOverride={async () => {
						try {
							await removeOverride.mutateAsync({
								entry_id: fileId,
								tag_id: et.tag.id
							});
						} catch (err) {
							console.error('Failed to clear override:', err);
							toast.error(`Failed to clear override: ${err}`);
						}
					}}
				/>
			))}
		</>
	);
}

interface EffectiveTagChipProps {
	tag: EffectiveTag;
	onRemoveDirect: () => void;
	onOverride: () => void;
	onClearOverride: () => void;
}

function EffectiveTagChip({
	tag,
	onRemoveDirect,
	onOverride,
	onClearOverride
}: EffectiveTagChipProps) {
	const color = tag.tag.color || '#3B82F6';
	const name = tag.tag.canonical_name;

	// `source` is the generated TagInheritanceSource union: Direct | Inherited | Overridden.
	if (tag.source === 'Direct') {
		return (
			<Tag color={color} size="sm" onRemove={onRemoveDirect}>
				{name}
			</Tag>
		);
	}

	if (tag.source === 'Overridden') {
		return (
			<span className="inline-flex items-center gap-1">
				<Tag color={color} size="sm" className="line-through opacity-40">
					{name}
					<span className="text-[9px] font-semibold uppercase tracking-wide opacity-80">
						overridden
					</span>
				</Tag>
				<button
					onClick={onClearOverride}
					title="Clear override and inherit this tag again"
					aria-label="Clear override"
					className="bg-app-box hover:bg-app-hover border-app-line text-ink-dull hover:text-ink rounded-full border p-1 transition-colors"
				>
					<ArrowCounterClockwise size={10} weight="bold" />
				</button>
			</span>
		);
	}

	// Inherited (depth > 0): greyed + provenance badge + override control.
	const depthLabel =
		tag.depth === 1 ? '1 level up' : `${tag.depth} levels up`;
	return (
		<span className="inline-flex items-center gap-1">
			<Tag
				color={color}
				size="sm"
				className={clsx('opacity-60')}
			>
				<span
					title={`Inherited from a parent folder (${depthLabel})`}
					className="text-[9px] font-semibold uppercase tracking-wide opacity-80"
				>
					↳ inherited
				</span>
				{name}
			</Tag>
			<button
				onClick={onOverride}
				title="Override (suppress) this inherited tag on this item"
				aria-label="Override inherited tag"
				className="bg-app-box hover:bg-app-hover border-app-line text-ink-dull hover:text-ink rounded-full border p-1 transition-colors"
			>
				<EyeSlash size={10} weight="bold" />
			</button>
		</span>
	);
}
