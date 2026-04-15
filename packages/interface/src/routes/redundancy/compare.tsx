/**
 * Volume Comparison View
 *
 * Select two volumes to see what's unique to each and what's shared.
 */

import { useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, ArrowsLeftRight, ShieldCheck } from "@phosphor-icons/react";
import { TopBarPortal, TopBarItem } from "../../TopBar";
import { CircleButton } from "@spacedrive/primitives";
import { useLibraryQuery } from "../../contexts/SpacedriveContext";
import type { SearchFilters, File as SdFile } from "@sd/ts-client";

function formatBytes(bytes: number): string {
	if (bytes === 0) return "0 B";
	const k = 1024;
	const sizes = ["B", "KB", "MB", "GB", "TB", "PB"];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

type CompareMode = "unique_a" | "unique_b" | "shared";

const NULL_FILTERS: SearchFilters = {
	file_types: null,
	tags: null,
	date_range: null,
	size_range: null,
	locations: null,
	content_types: null,
	include_hidden: null,
	include_archived: null,
	at_risk: null,
	on_volumes: null,
	not_on_volumes: null,
	min_volume_count: null,
	max_volume_count: null,
};

export function CompareVolumes() {
	const navigate = useNavigate();
	const [volumeA, setVolumeA] = useState<string | null>(null);
	const [volumeB, setVolumeB] = useState<string | null>(null);
	const [mode, setMode] = useState<CompareMode>("unique_a");

	// Fetch volume list for the picker
	const { data: summaryData } = useLibraryQuery({
		type: "redundancy.summary",
		input: {},
	});

	const volumes = summaryData?.volumes ?? [];

	// Build search filters based on compare mode
	const filters: SearchFilters | null = useMemo(() => {
		if (!volumeA || !volumeB) return null;

		switch (mode) {
			case "unique_a":
				return { ...NULL_FILTERS, on_volumes: [volumeA], not_on_volumes: [volumeB] };
			case "unique_b":
				return { ...NULL_FILTERS, on_volumes: [volumeB], not_on_volumes: [volumeA] };
			case "shared":
				return { ...NULL_FILTERS, on_volumes: [volumeA, volumeB], min_volume_count: 2 };
		}
	}, [volumeA, volumeB, mode]);

	// Search with redundancy filters
	const { data: searchData, isLoading: searchLoading } = useLibraryQuery(
		{
			type: "search.files",
			input: {
				query: "",
				scope: "Library",
				mode: "Fast",
				filters: filters ?? NULL_FILTERS,
				sort: { field: "Size", direction: "Desc" },
				pagination: { limit: 50, offset: 0 },
			},
		},
		{ enabled: !!filters },
	);

	const files = searchData?.files ?? [];
	const totalCount = searchData?.total_found ?? 0;

	const volumeAName =
		volumes.find((v) => v.volume_uuid === volumeA)?.display_name ??
		"Volume A";
	const volumeBName =
		volumes.find((v) => v.volume_uuid === volumeB)?.display_name ??
		"Volume B";

	const topBarContent = useMemo(
		() => (
			<div className="flex items-center gap-2">
				<CircleButton
					icon={ArrowLeft}
					title="Back to Redundancy"
					onClick={() => navigate("/redundancy")}
				/>
				<ArrowsLeftRight
					size={20}
					weight="bold"
					className="text-ink"
				/>
				<h1 className="text-xl font-bold text-ink">Compare Volumes</h1>
			</div>
		),
		[navigate],
	);

	return (
		<>
			<TopBarPortal
				left={
					<TopBarItem
						id="compare-title"
						label="Compare Volumes"
						priority="high"
					>
						{topBarContent}
					</TopBarItem>
				}
			/>

			<div className="flex h-full flex-col overflow-hidden">
				<div className="flex-1 overflow-auto p-4 space-y-4">
					{/* Volume Pickers */}
					<div className="flex items-center gap-3">
						<select
							value={volumeA ?? ""}
							onChange={(e) =>
								setVolumeA(e.target.value || null)
							}
							className="flex-1 rounded-lg border border-app-line bg-app-box/50 px-3 py-2 text-sm text-ink"
						>
							<option value="">Select Volume A</option>
							{volumes.map((v) => (
								<option
									key={v.volume_uuid}
									value={v.volume_uuid}
									disabled={v.volume_uuid === volumeB}
								>
									{v.display_name ?? v.volume_uuid} (
									{formatBytes(v.total_bytes)})
								</option>
							))}
						</select>

						<ArrowsLeftRight
							size={20}
							className="flex-shrink-0 text-ink-dull"
						/>

						<select
							value={volumeB ?? ""}
							onChange={(e) =>
								setVolumeB(e.target.value || null)
							}
							className="flex-1 rounded-lg border border-app-line bg-app-box/50 px-3 py-2 text-sm text-ink"
						>
							<option value="">Select Volume B</option>
							{volumes.map((v) => (
								<option
									key={v.volume_uuid}
									value={v.volume_uuid}
									disabled={v.volume_uuid === volumeA}
								>
									{v.display_name ?? v.volume_uuid} (
									{formatBytes(v.total_bytes)})
								</option>
							))}
						</select>
					</div>

					{/* Mode Toggle */}
					{volumeA && volumeB && (
						<div className="flex gap-1 rounded-lg border border-app-line bg-app-box/50 p-1">
							<ModeButton
								active={mode === "unique_a"}
								onClick={() => setMode("unique_a")}
								label={`Unique to ${volumeAName}`}
							/>
							<ModeButton
								active={mode === "shared"}
								onClick={() => setMode("shared")}
								label="Shared"
							/>
							<ModeButton
								active={mode === "unique_b"}
								onClick={() => setMode("unique_b")}
								label={`Unique to ${volumeBName}`}
							/>
						</div>
					)}

					{/* Results */}
					{!volumeA || !volumeB ? (
						<div className="flex flex-col items-center justify-center gap-2 py-16 text-ink-dull">
							<ShieldCheck size={48} weight="thin" />
							<span className="text-sm">
								Select two volumes to compare their contents
							</span>
						</div>
					) : searchLoading ? (
						<div className="flex items-center justify-center py-16 text-ink-dull">
							Loading comparison...
						</div>
					) : files.length === 0 ? (
						<div className="flex flex-col items-center justify-center gap-2 py-16 text-ink-dull">
							<ShieldCheck size={48} weight="thin" />
							<span className="text-sm">
								{mode === "shared"
									? "No shared files found between these volumes"
									: `No unique files found on ${mode === "unique_a" ? volumeAName : volumeBName}`}
							</span>
						</div>
					) : (
						<div>
							<div className="mb-2 text-xs text-ink-dull">
								{totalCount.toLocaleString()} files found
							</div>
							<div className="rounded-lg border border-app-line">
								<table className="w-full text-sm">
									<thead className="bg-app text-left text-xs text-ink-dull">
										<tr>
											<th className="px-4 py-2 font-medium">
												Name
											</th>
											<th className="px-4 py-2 font-medium">
												Size
											</th>
											<th className="px-4 py-2 font-medium">
												Extension
											</th>
										</tr>
									</thead>
									<tbody>
										{files.map((file: SdFile) => (
											<tr
												key={file.id}
												className="border-t border-app-line/50 text-ink transition-colors hover:bg-app-hover/50"
											>
												<td className="max-w-[400px] truncate px-4 py-2 font-medium">
													{file.name}
												</td>
												<td className="px-4 py-2 text-ink-dull">
													{formatBytes(file.size)}
												</td>
												<td className="px-4 py-2 text-ink-dull">
													{file.extension || "\u2014"}
												</td>
											</tr>
										))}
									</tbody>
								</table>
							</div>
						</div>
					)}
				</div>
			</div>
		</>
	);
}

function ModeButton({
	active,
	onClick,
	label,
}: {
	active: boolean;
	onClick: () => void;
	label: string;
}) {
	return (
		<button
			onClick={onClick}
			className={`flex-1 rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
				active
					? "bg-accent text-white"
					: "text-ink-dull hover:text-ink"
			}`}
		>
			{label}
		</button>
	);
}
