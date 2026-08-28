import type {OrganizeProgressSummary} from '@sd/ts-client';
import {progressSegments} from './decision/contracts';

export function OrganizeProgress({progress}: {progress: OrganizeProgressSummary}) {
	const marked = progress.total_units - progress.unmarked_units;
	const percent = progress.total_units === 0 ? 0 : Math.round((marked / progress.total_units) * 100);

	return (
		<section aria-label="Organize progress" className="space-y-1.5">
			<div className="flex items-center justify-between text-xs text-ink-dull">
				<span>{marked} / {progress.total_units} marked</span>
				<span>{percent}%</span>
			</div>
			<div className="flex h-2 overflow-hidden rounded-full bg-app-box" data-testid="organize-progress-bar">
				{progressSegments(progress).map((segment) => (
					<div key={segment.kind} data-progress-kind={segment.kind} style={{width: `${segment.fraction * 100}%`}} />
				))}
			</div>
			{progress.unmarked_units > 0 && (
				<p className="text-xs text-amber-300">{progress.unmarked_units} items are not marked yet.</p>
			)}
		</section>
	);
}
