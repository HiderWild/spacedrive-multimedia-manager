import type { ReactNode } from "react";

export function OrganizeLayout(props: { left: ReactNode; center: ReactNode; right: ReactNode }) {
	return (
		<div className="grid h-full min-h-0 grid-cols-[280px_minmax(0,1fr)_360px] gap-2 p-2">
			<section className="min-h-0 overflow-hidden rounded-xl border border-app-line bg-app/70">{props.left}</section>
			<section className="min-h-0 overflow-hidden rounded-xl border border-app-line bg-app/70">{props.center}</section>
			<section className="min-h-0 overflow-hidden rounded-xl border border-app-line bg-app/70">{props.right}</section>
		</div>
	);
}
