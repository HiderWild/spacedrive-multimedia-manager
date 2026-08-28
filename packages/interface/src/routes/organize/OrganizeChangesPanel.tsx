import type {OrganizeCommitPlanOutput} from '@sd/ts-client';
import {buildCommitReview} from './OrganizeCommitDialog';

export function OrganizeChangesPanel({plan}: {plan: OrganizeCommitPlanOutput | undefined}) {
	if (!plan) return <p className="text-sm text-ink-dull">Loading commit plan…</p>;
	const review = buildCommitReview(plan);
	return <section className="border-b border-app-line bg-app-box/20 px-4 py-2 text-xs" aria-label="Organize changes">
		<div className="flex flex-wrap gap-x-4 gap-y-1 text-ink-dull"><span>Revision {review.revision}</span><span>{plan.discard_roots.length} discard roots</span><span>{plan.move_groups.length} move groups</span><span>{plan.keep_units} keep units</span><span>{plan.unmarked_units} unmarked</span></div>
		{review.blockingReasons.length > 0 && <ul className="mt-1 list-disc pl-4 text-amber-300">{review.blockingReasons.map((reason) => <li key={reason}>{reason}</li>)}</ul>}
	</section>;
}
