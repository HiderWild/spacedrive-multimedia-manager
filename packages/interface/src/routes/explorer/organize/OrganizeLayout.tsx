import type {ReactNode} from 'react';

export function OrganizeLayout(props: {
	left: ReactNode;
	center: ReactNode;
	right: ReactNode;
}) {
	return (
		<div className="grid h-full min-h-0 grid-cols-[280px_minmax(0,1fr)_400px] gap-2 p-2">
			<section className="border-app-line bg-app/70 min-h-0 overflow-hidden rounded-xl border">
				{props.left}
			</section>
			<section className="border-app-line bg-app/70 min-h-0 overflow-hidden rounded-xl border">
				{props.center}
			</section>
			<section className="border-app-line bg-app/70 min-h-0 overflow-hidden rounded-xl border">
				{props.right}
			</section>
		</div>
	);
}
