import clsx from "clsx";
import { Fragment, type ReactNode, useRef } from "react";

import { PaneResizer } from "./PaneResizer";
import { SecondaryPane } from "./SecondaryPane";
import { PRIMARY_PANE_ID, type PaneDescriptor } from "./types";

interface PaneLayoutProps {
	panes: PaneDescriptor[];
	sizes: number[];
	focusedId: string;
	onFocus: (id: string) => void;
	onClose: (id: string) => void;
	onResize: (boundaryIndex: number, leftPercent: number, rightPercent: number) => void;
	/** Content for the primary pane, rendered with the shared explorer context. */
	primary: ReactNode;
}

/**
 * Arranges 1..N explorer panes in a horizontal split with draggable dividers.
 *
 * With a single pane it renders `primary` directly with no wrappers, so the
 * default explorer is byte-for-byte unchanged. With more panes it lays them out
 * in a flex row: pane 0 hosts the shared-context `primary`, the rest are
 * isolated `SecondaryPane`s, separated by `PaneResizer`s.
 */
export function PaneLayout({
	panes,
	sizes,
	focusedId,
	onFocus,
	onClose,
	onResize,
	primary,
}: PaneLayoutProps) {
	const containerRef = useRef<HTMLDivElement>(null);

	if (panes.length <= 1) {
		return <>{primary}</>;
	}

	const handleResizerDrag = (boundaryIndex: number) => (clientX: number) => {
		const el = containerRef.current;
		if (!el) return;
		const rect = el.getBoundingClientRect();
		if (rect.width <= 0) return;

		const combined = sizes[boundaryIndex] + sizes[boundaryIndex + 1];
		let leftOffset = 0;
		for (let i = 0; i < boundaryIndex; i++) leftOffset += sizes[i];

		const pointerPercent = ((clientX - rect.left) / rect.width) * 100;
		let leftPercent = pointerPercent - leftOffset;
		leftPercent = Math.max(0, Math.min(combined, leftPercent));
		const rightPercent = combined - leftPercent;
		onResize(boundaryIndex, leftPercent, rightPercent);
	};

	return (
		<div ref={containerRef} className="flex h-full w-full overflow-hidden">
			{panes.map((pane, i) => (
				<Fragment key={pane.id}>
					<div
						className={clsx(
							"relative flex min-w-0 flex-col overflow-hidden outline-none",
							focusedId === pane.id &&
								"ring-1 ring-inset ring-accent",
						)}
						style={{
							flexBasis: `${sizes[i]}%`,
							flexGrow: 0,
							flexShrink: 0,
						}}
						onMouseDownCapture={() => onFocus(pane.id)}
					>
						{pane.id === PRIMARY_PANE_ID ? (
							primary
						) : (
							<SecondaryPane
								descriptor={pane}
								onClose={() => onClose(pane.id)}
							/>
						)}
					</div>
					{i < panes.length - 1 && (
						<PaneResizer onDrag={handleResizerDrag(i)} />
					)}
				</Fragment>
			))}
		</div>
	);
}
