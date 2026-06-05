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
}) {
	const { t } = useTranslation("explorer");
	const items = props.leftTab === "keep" ? props.keepFiles : props.discardFiles;

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex gap-1 border-b border-app-line p-2">
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
