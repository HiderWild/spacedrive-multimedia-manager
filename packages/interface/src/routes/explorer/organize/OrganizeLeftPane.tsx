import {CaretLeft} from '@phosphor-icons/react';
import { useTranslation } from "react-i18next";
import type { File } from "@sd/ts-client";
import { File as FileComponent } from "../File";
import type { OrganizeLeftTab } from "./organizeTypes";

export function OrganizeLeftPane(props: {
	leftTab: OrganizeLeftTab;
	onLeftTabChange: (tab: OrganizeLeftTab) => void;
	keepFiles: File[];
	discardFiles: File[];
	onRevealItem: (file: File) => void;
	onDeleteClick?: () => void;
	onTogglePane?: () => void;
}) {
	const { t } = useTranslation("explorer");
	const items = props.leftTab === "keep" ? props.keepFiles : props.discardFiles;

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex gap-1 border-b border-app-line p-2">
				{/* Hide button */}
				{props.onTogglePane && (
					<button
						onClick={props.onTogglePane}
						className="hover:bg-app-box rounded-md p-2 text-ink-dull transition-colors hover:text-ink"
						title="Hide Keep/Discard panel"
					>
						<CaretLeft size={16} weight="bold" />
					</button>
				)}
				<button
					className={`flex-1 rounded-md px-3 py-2 text-sm ${props.leftTab === "keep" ? "bg-accent/15 text-accent" : "text-ink-dull hover:bg-app-box"}`}
					onClick={() => props.onLeftTabChange("keep")}
				>
					{t("organize.keepTab")}
				</button>
				<button
					className={`flex-1 rounded-md px-3 py-2 text-sm ${props.leftTab === "discard" ? "bg-accent/15 text-accent" : "text-ink-dull hover:bg-app-box"}`}
					onClick={() => props.onLeftTabChange("discard")}
				>
					{t("organize.discardTab")}
				</button>
			</div>
			{props.leftTab === "discard" ? (
				<div className="border-b border-app-line p-2">
					<button
						className="w-full rounded-md bg-rose-500/15 px-3 py-2 text-sm text-rose-300 disabled:opacity-40"
						disabled={props.discardFiles.length === 0}
						onClick={props.onDeleteClick}
					>
						{t("organize.deleteNow")}
					</button>
				</div>
			) : null}
			<div className="min-h-0 flex-1 overflow-auto p-2">
				{items.map((file) => (
					<button
						key={file.id}
						className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-app-box"
						onClick={() => props.onRevealItem(file)}
					>
						<FileComponent.Thumb file={file} size={32} />
						<span className="truncate text-sm text-ink">{file.name}</span>
					</button>
				))}
			</div>
		</div>
	);
}
