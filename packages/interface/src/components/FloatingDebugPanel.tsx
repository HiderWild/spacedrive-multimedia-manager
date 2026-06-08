import {Minus, X} from '@phosphor-icons/react';
import clsx from 'clsx';
import {useCallback, useEffect, useRef, useState} from 'react';

interface FloatingDebugPanelProps {
	children: React.ReactNode;
	initialPosition?: {top: number; right: number};
	onClose: () => void;
	title?: string;
}

export function FloatingDebugPanel({
	children,
	initialPosition = {top: 16, right: 16},
	onClose,
	title = 'Debug Panel'
}: FloatingDebugPanelProps) {
	const panelRef = useRef<HTMLDivElement>(null);
	const containerRef = useRef<HTMLDivElement>(null);
	const [position, setPosition] = useState(initialPosition);
	const [isDragging, setIsDragging] = useState(false);
	const [dragOffset, setDragOffset] = useState({x: 0, y: 0});

	// Load collapsed state from localStorage
	const [isCollapsed, setIsCollapsed] = useState(() => {
		const stored = localStorage.getItem('organize-debug-panel-collapsed');
		return stored === 'true';
	});

	// Persist collapsed state to localStorage
	useEffect(() => {
		localStorage.setItem('organize-debug-panel-collapsed', String(isCollapsed));
	}, [isCollapsed]);

	const handleMouseDown = (e: React.MouseEvent) => {
		if ((e.target as HTMLElement).closest('button')) {
			return;
		}

		const rect = panelRef.current?.getBoundingClientRect();
		if (!rect) return;

		setIsDragging(true);
		setDragOffset({
			x: e.clientX - rect.left,
			y: e.clientY - rect.top
		});
	};

	const handleMouseMove = useCallback(
		(e: MouseEvent) => {
			if (!isDragging || !panelRef.current) return;

			const panel = panelRef.current;
			const container = panel.parentElement;
			if (!container) return;

			const containerRect = container.getBoundingClientRect();
			const panelRect = panel.getBoundingClientRect();

			let newLeft = e.clientX - containerRect.left - dragOffset.x;
			let newTop = e.clientY - containerRect.top - dragOffset.y;

			// Clamp within container bounds
			newLeft = Math.max(0, Math.min(newLeft, containerRect.width - panelRect.width));
			newTop = Math.max(0, Math.min(newTop, containerRect.height - panelRect.height));

			// Convert left position to right-based positioning
			const newRight = containerRect.width - newLeft - panelRect.width;

			setPosition({top: newTop, right: newRight});
		},
		[isDragging, dragOffset]
	);

	const handleMouseUp = useCallback(() => {
		setIsDragging(false);
	}, []);

	useEffect(() => {
		if (isDragging) {
			window.addEventListener('mousemove', handleMouseMove);
			window.addEventListener('mouseup', handleMouseUp);
			return () => {
				window.removeEventListener('mousemove', handleMouseMove);
				window.removeEventListener('mouseup', handleMouseUp);
			};
		}
	}, [isDragging, handleMouseMove, handleMouseUp]);

	// Handle ESC key to close
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.key === 'Escape') {
				onClose();
			}
		};

		window.addEventListener('keydown', handleKeyDown);
		return () => window.removeEventListener('keydown', handleKeyDown);
	}, [onClose]);

	return (
		<div
			ref={panelRef}
			className={clsx(
				'absolute z-50',
				'bg-app-box/95 backdrop-blur-md',
				'border border-app-line rounded-lg',
				'shadow-lg',
				'overflow-hidden',
				'w-[300px]',
				isDragging && 'cursor-move'
			)}
			style={{
				top: `${position.top}px`,
				right: `${position.right}px`,
				minHeight: isCollapsed ? '40px' : '100px',
				maxHeight: isCollapsed ? '40px' : '400px'
			}}
		>
			{/* Title bar (draggable) */}
			<div
				className={clsx(
					'flex items-center justify-between gap-2 px-3 py-2',
					'border-b border-app-line',
					'cursor-move select-none',
					'bg-app-box/50'
				)}
				onMouseDown={handleMouseDown}
				aria-label="Drag to move panel"
			>
				<span className="text-ink text-xs font-semibold">{title}</span>
				<div className="flex items-center gap-1">
					<button
						type="button"
						onClick={() => setIsCollapsed(!isCollapsed)}
						className="text-ink-dull hover:text-ink hover:bg-app-hover rounded p-1 transition-colors"
						aria-label={isCollapsed ? 'Expand panel' : 'Collapse panel'}
						title={isCollapsed ? 'Expand' : 'Collapse'}
					>
						<Minus size={12} weight="bold" />
					</button>
					<button
						type="button"
						onClick={onClose}
						className="text-ink-dull hover:text-ink hover:bg-app-hover rounded p-1 transition-colors"
						aria-label="Close debug panel"
						title="Close"
					>
						<X size={12} weight="bold" />
					</button>
				</div>
			</div>

			{/* Content area (scrollable when not collapsed) */}
			{!isCollapsed && (
				<div className="overflow-auto p-2" style={{maxHeight: '360px'}}>
					{children}
				</div>
			)}
		</div>
	);
}
