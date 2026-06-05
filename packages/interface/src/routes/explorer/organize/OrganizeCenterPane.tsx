import clsx from "clsx";
import { CheckCircle, XCircle } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import type { File } from "@sd/ts-client";
import { File as FileComponent } from "../File";
import type { OrganizeDecision, OrganizeCenterLayout } from "./organizeTypes";
import type { OrganizePresentationEntry } from "./organizeState";

export function OrganizeCenterPane(props: {
	layout: OrganizeCenterLayout;
	onLayoutChange: (layout: OrganizeCenterLayout) => void;
	presentation: OrganizePresentationEntry[];
	selectedFileId: string | null;
	onSelectFile: (file: File) => void;
	onMarkKeep: (file: File) => void;
	onMarkDiscard: (file: File) => void;
	onClearDecision: (file: File) => void;
}) {
	const { t } = useTranslation("explorer");
	const selected = props.presentation.find((item) => item.file.id === props.selectedFileId)?.file ?? null;

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex items-center gap-2 border-b border-app-line px-3 py-2">
				<button
					className="rounded-md bg-emerald-500/15 px-3 py-1.5 text-sm text-emerald-300 disabled:opacity-40"
					disabled={!selected}
					onClick={() => selected && props.onMarkKeep(selected)}
				>
					{t("organize.keepAction")}
				</button>
				<button
					className="rounded-md bg-rose-500/15 px-3 py-1.5 text-sm text-rose-300 disabled:opacity-40"
					disabled={!selected}
					onClick={() => selected && props.onMarkDiscard(selected)}
				>
					{t("organize.discardAction")}
				</button>
				<button
					className="rounded-md bg-app-box px-3 py-1.5 text-sm text-ink disabled:opacity-40"
					disabled={!selected}
					onClick={() => selected && props.onClearDecision(selected)}
				>
					{t("organize.clearAction")}
				</button>
			</div>
			<div
				className={clsx(
					"min-h-0 flex-1 overflow-auto p-3",
					props.layout === "grid"
						? "grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3"
						: "flex flex-col gap-2",
				)}
			>
				{props.presentation.map((item) => (
					<button
						key={item.file.id}
						data-file-id={item.file.id}
						onClick={() => props.onSelectFile(item.file)}
						className={clsx(
							"relative rounded-xl border border-app-line bg-app-box/60 p-3 text-left",
							item.dimmed && "opacity-50",
							item.file.id === props.selectedFileId && "ring-2 ring-accent",
						)}
					>
						<FileComponent.Thumb file={item.file} size={props.layout === "grid" ? 96 : 48} />
						<div className="mt-2 truncate text-sm text-ink">{item.file.name}</div>
						{item.decision === "keep" ? (
							<CheckCircle className="absolute bottom-2 right-2 text-emerald-400" size={20} weight="fill" />
						) : item.decision === "discard" ? (
							<XCircle className="absolute bottom-2 right-2 text-rose-400" size={20} weight="fill" />
						) : null}
					</button>
				))}
			</div>
		</div>
	);
}
