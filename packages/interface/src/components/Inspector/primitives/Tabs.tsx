import type {Icon} from '@phosphor-icons/react';
import clsx from 'clsx';
import {motion} from 'framer-motion';
import {useState} from 'react';

interface Tab {
	id: string;
	label: string;
	icon: Icon;
	badge?: number;
	disabled?: boolean;
	tooltip?: string;
}

interface TabsProps {
	tabs: Tab[];
	activeTab: string;
	onChange: (tabId: string) => void;
	className?: string;
}

export function Tabs({tabs, activeTab, onChange, className}: TabsProps) {
	const [hoveredTab, setHoveredTab] = useState<string | null>(null);

	return (
		<div
			className={clsx(
				'bg-app-box/50 flex gap-0.5 rounded-lg p-0.5',
				className
			)}
		>
			{tabs.map((tab) => {
				const Icon = tab.icon;
				const isActive = activeTab === tab.id;
				const isHovered = hoveredTab === tab.id;

				return (
					<div key={tab.id} className="relative">
						<button
							onClick={() => {
								if (!tab.disabled) {
									onChange(tab.id);
								}
							}}
							onMouseEnter={() => setHoveredTab(tab.id)}
							onMouseLeave={() => setHoveredTab(null)}
							aria-disabled={tab.disabled}
							className={clsx(
								'relative rounded-md p-2 transition-all',
								'focus:ring-accent focus:outline-none focus:ring-1',
								tab.disabled && 'cursor-not-allowed opacity-40',
								isActive
									? 'text-sidebar-ink'
									: 'text-sidebar-inkDull hover:text-sidebar-ink'
							)}
							title={tab.tooltip ?? tab.label}
						>
							{isActive && (
								<motion.div
									layoutId="activeTab"
									className="bg-sidebar-selected/60 absolute inset-0 rounded-md"
									transition={{
										duration: 0.2,
										ease: [0.25, 1, 0.5, 1]
									}}
								/>
							)}
							<div className="relative z-10 flex items-center justify-center">
								<Icon className="size-4" weight="bold" />
								{tab.badge !== undefined && tab.badge > 0 && (
									<span className="bg-accent absolute -right-1.5 -top-1.5 flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-[9px] font-bold text-white">
										{tab.badge}
									</span>
								)}
							</div>
						</button>

						{/* Tooltip */}
						{isHovered && !isActive && (
							<motion.div
								initial={{opacity: 0, y: -5}}
								animate={{opacity: 1, y: 0}}
								transition={{delay: 0.3}}
								className="bg-app-box border-app-line pointer-events-none absolute left-1/2 top-full z-50 mt-1 -translate-x-1/2 whitespace-nowrap rounded-md border px-2 py-1 shadow-lg"
							>
								<span className="text-sidebar-ink text-xs font-medium">
									{tab.tooltip ?? tab.label}
								</span>
							</motion.div>
						)}
					</div>
				);
			})}
		</div>
	);
}
