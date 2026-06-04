import type {Icon} from '@phosphor-icons/react';
import {
	Camera,
	ChartPieSlice,
	Columns,
	GridFour,
	Rows,
	Sparkle,
	SquaresFour,
	SquareHalf
} from '@phosphor-icons/react';
import {CircleButton} from '@spacedrive/primitives';
import clsx from 'clsx';
import {i18n} from '../../../i18n';
import {AnimatePresence, motion} from 'framer-motion';
import {useEffect, useRef, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {createPortal} from 'react-dom';

type ViewMode = 'list' | 'grid' | 'column' | 'media' | 'masonry' | 'size' | 'knowledge';

interface ViewOption {
	id: ViewMode | 'timeline';
	label: string;
	icon: Icon;
	color: string;
	keybind: string;
}

const viewOptions: ViewOption[] = [
	{
		id: 'grid',
		label: i18n.t('viewModes.grid', { ns: 'explorer' }),
		icon: GridFour,
		color: 'bg-accent',
		keybind: '⌘1'
	},
	{
		id: 'list',
		label: i18n.t('viewModes.list', { ns: 'explorer' }),
		icon: Rows,
		color: 'bg-purple-500',
		keybind: '⌘2'
	},
	{
		id: 'media',
		label: i18n.t('viewModes.media', { ns: 'explorer' }),
		icon: Camera,
		color: 'bg-pink-500',
		keybind: '⌘3'
	},
	{
		id: 'masonry',
		label: i18n.t('viewModes.masonry', { ns: 'explorer' }),
		icon: SquareHalf,
		color: 'bg-teal-500',
		keybind: '⌘7'
	},
	{
		id: 'column',
		label: i18n.t('viewModes.column', { ns: 'explorer' }),
		icon: Columns,
		color: 'bg-orange-500',
		keybind: '⌘4'
	},
	{
		id: 'size',
		label: i18n.t('viewModes.size', { ns: 'explorer' }),
		icon: ChartPieSlice,
		color: 'bg-green-500',
		keybind: '⌘5'
	},
	{
		id: 'knowledge',
		label: i18n.t('viewModes.knowledge', { ns: 'explorer' }),
		icon: Sparkle,
		color: 'bg-purple-500',
		keybind: '⌘6'
	}
	// {
	// 	id: "timeline",
	// 	label: "Timeline",
	// 	icon: Clock,
	// 	color: "bg-yellow-500",
	// 	keybind: "⌘7",
	// },
];

interface ViewModeMenuPanelProps {
	viewMode: ViewMode;
	onViewModeChange: (mode: ViewMode) => void;
	onClose?: () => void;
}

export function ViewModeMenuPanel({
	viewMode,
	onViewModeChange,
	onClose
}: ViewModeMenuPanelProps) {
	const availableViews = viewOptions.filter(
		(option) => option.id !== 'knowledge' || import.meta.env.DEV
	);

	return (
		<div className="bg-app border-app-line w-[240px] rounded-lg border p-2 shadow-2xl">
			<div className="grid grid-cols-3 gap-1">
				{availableViews.map((option) => (
					<button
						key={`${option.id}-${option.label}`}
						onClick={() => {
							if (option.id !== 'timeline') {
								onViewModeChange(option.id as ViewMode);
							}
							onClose?.();
						}}
						className={clsx(
							'flex flex-col items-center gap-1.5 rounded-md px-2 py-2',
							option.id === 'timeline' &&
								'cursor-not-allowed opacity-50',
							viewMode === option.id
								? 'bg-app-selected'
								: 'hover:bg-app-hover/50'
						)}
					>
						<option.icon
							className="size-6 text-white"
							weight={viewMode === option.id ? 'fill' : 'bold'}
						/>
						<div className="flex flex-col items-center gap-0.5">
							<div className="text-menu-ink text-xs font-medium">
								{option.label}
							</div>
							<div className="text-menu-faint text-[10px]">
								{option.keybind}
							</div>
						</div>
					</button>
				))}
			</div>
		</div>
	);
}

interface ViewModeMenuProps {
	viewMode: ViewMode;
	onViewModeChange: (mode: ViewMode) => void;
}

export function ViewModeMenu({viewMode, onViewModeChange}: ViewModeMenuProps) {
	const {t} = useTranslation('explorer');
	const [isOpen, setIsOpen] = useState(false);
	const buttonRef = useRef<HTMLButtonElement>(null);
	const panelRef = useRef<HTMLDivElement>(null);
	const [position, setPosition] = useState({top: 0, right: 0});

	useEffect(() => {
		if (isOpen && buttonRef.current) {
			const rect = buttonRef.current.getBoundingClientRect();
			setPosition({
				top: rect.bottom + 8,
				right: window.innerWidth - rect.right
			});
		}
	}, [isOpen]);

	useEffect(() => {
		const handleClickOutside = (e: MouseEvent) => {
			if (
				panelRef.current &&
				buttonRef.current &&
				!panelRef.current.contains(e.target as Node) &&
				!buttonRef.current.contains(e.target as Node)
			) {
				setIsOpen(false);
			}
		};

		if (isOpen) {
			document.addEventListener('mousedown', handleClickOutside);
			return () =>
				document.removeEventListener('mousedown', handleClickOutside);
		}
	}, [isOpen]);

	return (
		<>
			<CircleButton
				ref={buttonRef}
				icon={SquaresFour}
				onClick={() => setIsOpen(!isOpen)}
				active={isOpen}
			>
				{t('topBar.views')}
			</CircleButton>

			{isOpen &&
				createPortal(
					<AnimatePresence>
						<motion.div
							ref={panelRef}
							initial={{opacity: 0, y: -10}}
							animate={{opacity: 1, y: 0}}
							exit={{opacity: 0, y: -10}}
							transition={{duration: 0.15}}
							style={{
								position: 'fixed',
								top: `${position.top}px`,
								right: `${position.right}px`
							}}
							className="z-50"
						>
							<ViewModeMenuPanel
								viewMode={viewMode}
								onViewModeChange={onViewModeChange}
								onClose={() => setIsOpen(false)}
							/>
						</motion.div>
					</AnimatePresence>,
					document.body
				)}
		</>
	);
}
