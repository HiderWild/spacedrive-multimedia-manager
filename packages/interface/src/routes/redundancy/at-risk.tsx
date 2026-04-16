/**
 * At-Risk Files View
 *
 * Paginated list of files that exist on only one volume,
 * sorted by size (biggest risk first).
 */

import { useState, useMemo } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import { ShieldWarning, ArrowLeft, CaretLeft, CaretRight } from "@phosphor-icons/react";
import { TopBarPortal, TopBarItem } from "../../TopBar";
import { CircleButton } from "@spacedrive/primitives";
import { useLibraryQuery } from "../../contexts/SpacedriveContext";
import type { File as SdFile } from "@sd/ts-client";

function formatBytes(bytes: number): string {
	if (bytes === 0) return "0 B";
	const k = 1024;
	const sizes = ["B", "KB", "MB", "GB", "TB", "PB"];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

const PAGE_SIZE = 50;

export function AtRiskFiles() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const volumeFilter = searchParams.get("volume");
	const atRiskParam = searchParams.get("at_risk");
	const isAtRisk = atRiskParam !== "false"; // default true

	const [offset, setOffset] = useState(0);

	const { data, isLoading } = useLibraryQuery({
		type: "search.files",
		input: {
			query: "",
			scope: "Library",
			mode: "Fast",
			filters: {
				file_types: null,
				tags: null,
				date_range: null,
				size_range: null,
				locations: null,
				content_types: null,
				include_hidden: null,
				include_archived: null,
				at_risk: isAtRisk,
				on_volumes: volumeFilter ? [volumeFilter] : null,
				not_on_volumes: null,
				min_volume_count: null,
				max_volume_count: null,
			},
			sort: { field: "Size", direction: "Desc" },
			pagination: { limit: PAGE_SIZE, offset },
		},
	});

	const files = data?.files ?? [];
	const totalCount = data?.total_found ?? 0;
	const hasMore = offset + PAGE_SIZE < totalCount;
	const hasPrev = offset > 0;
	const currentPage = Math.floor(offset / PAGE_SIZE) + 1;
	const totalPages = Math.max(1, Math.ceil(totalCount / PAGE_SIZE));

	const topBarContent = useMemo(
		() => (
			<div className="flex items-center gap-2">
				<CircleButton
					icon={ArrowLeft}
					title="Back to Redundancy"
					onClick={() => navigate("/redundancy")}
				/>
				<ShieldWarning
					size={20}
					weight="bold"
					className="text-status-warning"
				/>
				<h1 className="text-xl font-bold text-ink">
					{isAtRisk ? "At-Risk" : "Redundant"} Files
				</h1>
				{totalCount > 0 && (
					<span className="text-sm text-ink-dull">
						({totalCount.toLocaleString()} files)
					</span>
				)}
			</div>
		),
		[navigate, isAtRisk, totalCount],
	);

	return (
		<>
			<TopBarPortal
				left={
					<TopBarItem
						id="at-risk-title"
						label="At-Risk Files"
						priority="high"
					>
						{topBarContent}
					</TopBarItem>
				}
			/>

			<div className="flex h-full flex-col overflow-hidden">
				{isLoading ? (
					<div className="flex flex-1 items-center justify-center text-ink-dull">
						Loading files...
					</div>
				) : files.length === 0 ? (
					<div className="flex flex-1 flex-col items-center justify-center gap-2 text-ink-dull">
						<ShieldWarning size={48} weight="thin" />
						<span className="text-sm">
							{isAtRisk
								? "No at-risk files found. All your data is safely replicated!"
								: "No redundant files found."}
						</span>
					</div>
				) : (
					<>
						{/* File List */}
						<div className="flex-1 overflow-auto">
							<table className="w-full text-sm">
								<thead className="sticky top-0 bg-app text-left text-xs text-ink-dull">
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
										<th className="px-4 py-2 font-medium">
											Modified
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
											<td className="px-4 py-2 text-ink-dull">
												{new Date(
													file.modified_at,
												).toLocaleDateString()}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>

						{/* Pagination */}
						{totalPages > 1 && (
							<div className="flex items-center justify-between border-t border-app-line px-4 py-2">
								<span className="text-xs text-ink-dull">
									Page {currentPage} of {totalPages}
								</span>
								<div className="flex gap-1">
									<button
										onClick={() =>
											setOffset(
												Math.max(0, offset - PAGE_SIZE),
											)
										}
										disabled={!hasPrev}
										className="rounded p-1 text-ink-dull transition-colors hover:bg-app-hover disabled:opacity-30"
									>
										<CaretLeft size={16} />
									</button>
									<button
										onClick={() =>
											setOffset(offset + PAGE_SIZE)
										}
										disabled={!hasMore}
										className="rounded p-1 text-ink-dull transition-colors hover:bg-app-hover disabled:opacity-30"
									>
										<CaretRight size={16} />
									</button>
								</div>
							</div>
						)}
					</>
				)}
			</div>
		</>
	);
}
