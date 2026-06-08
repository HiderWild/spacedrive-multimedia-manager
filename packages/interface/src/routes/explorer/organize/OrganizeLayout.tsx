import {CaretRight} from '@phosphor-icons/react';
import type {ReactNode} from 'react';

export function OrganizeLayout(props: {
	left: ReactNode;
	center: ReactNode;
	right?: ReactNode;
	showLeftPane: boolean;
	onToggleLeftPane: () => void;
}) {
	return (
		<div
			className={
				props.right
					? props.showLeftPane
						? 'grid h-full min-h-0 grid-cols-[280px_minmax(0,1fr)_400px] gap-2 p-2'
						: 'grid h-full min-h-0 grid-cols-[minmax(0,1fr)_400px] gap-2 p-2'
					: props.showLeftPane
						? 'grid h-full min-h-0 grid-cols-[280px_minmax(0,1fr)] gap-2 p-2'
						: 'grid h-full min-h-0 grid-cols-[minmax(0,1fr)] gap-2 p-2'
			}
		>
			{props.showLeftPane && (
				<section className="border-app-line bg-app/70 min-h-0 overflow-hidden rounded-xl border">
					{props.left}
				</section>
			)}
			<section className="border-app-line bg-app/70 relative min-h-0 overflow-hidden rounded-xl border">
				{/* Show button when left pane is hidden */}
				{!props.showLeftPane && (
					<button
						onClick={props.onToggleLeftPane}
						className="hover:bg-app-box border-app-line absolute left-2 top-2 z-10 rounded-md border bg-app/80 p-1.5 text-ink-dull backdrop-blur-sm transition-colors hover:text-ink"
						title="Show Keep/Discard panel"
					>
						<CaretRight size={16} weight="bold" />
					</button>
				)}
				{props.center}
			</section>
			{props.right && (
				<section className="border-app-line bg-app/70 min-h-0 overflow-hidden rounded-xl border">
					{props.right}
				</section>
			)}
		</div>
	);
}
