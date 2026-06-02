import { useCallback, useRef } from "react";

interface PaneResizerProps {
	/** Called with the pointer's clientX while dragging the divider. */
	onDrag: (clientX: number) => void;
}

/**
 * A thin draggable divider between two panes.
 *
 * Hand-rolled (no external split library) to keep the dependency footprint
 * unchanged. It only reports the pointer position; `PaneLayout` converts that
 * into pane size percentages against the container width.
 */
export function PaneResizer({ onDrag }: PaneResizerProps) {
	const draggingRef = useRef(false);

	const handlePointerDown = useCallback(
		(e: React.PointerEvent) => {
			e.preventDefault();
			draggingRef.current = true;

			const handleMove = (ev: PointerEvent) => {
				if (draggingRef.current) onDrag(ev.clientX);
			};
			const handleUp = () => {
				draggingRef.current = false;
				window.removeEventListener("pointermove", handleMove);
				window.removeEventListener("pointerup", handleUp);
				document.body.style.cursor = "";
				document.body.style.userSelect = "";
			};

			window.addEventListener("pointermove", handleMove);
			window.addEventListener("pointerup", handleUp);
			document.body.style.cursor = "col-resize";
			document.body.style.userSelect = "none";
		},
		[onDrag],
	);

	return (
		<div
			role="separator"
			aria-orientation="vertical"
			onPointerDown={handlePointerDown}
			className="group relative z-10 w-px shrink-0 cursor-col-resize bg-app-line"
		>
			<div className="absolute inset-y-0 -left-1 -right-1 transition-colors group-hover:bg-accent/30" />
		</div>
	);
}
