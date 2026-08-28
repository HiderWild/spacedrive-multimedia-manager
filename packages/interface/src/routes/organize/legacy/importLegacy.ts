import type {
	JobInfoOutput,
	Model,
	OrganizeChildrenInput,
	OrganizeCreateInput,
	OrganizeCreateOutcome,
	OrganizeDecisionOutcome,
	OrganizeGetInput,
	OrganizeGetOutput,
	OrganizeSetDecisionInput,
	SdPath,
} from '@sd/ts-client';
import type {Platform} from '../../../contexts/PlatformContext';
import type {
	LegacyImportApi,
	LegacyImportFailure,
	LegacyImportResult,
	LegacyOrganizeRecord,
	LegacyOrganizeState,
	LegacyOrganizeStateSummary,
} from './types';

const CHILD_PAGE_SIZE = 200;
const SNAPSHOT_POLL_MS = 100;
const SNAPSHOT_TIMEOUT_MS = 10 * 60 * 1000;

/** Normalizes physical paths using Windows comparison rules used by legacy JSON. */
export function normalizeLegacyPath(path: string): string {
	const input = path.trim().replaceAll('\\', '/');
	const isUnc = input.startsWith('//');
	const drive = /^[a-zA-Z]:/.test(input) ? input.slice(0, 2).toLowerCase() : '';
	const rooted = input.startsWith('/') && !isUnc;
	const segments = (drive ? input.slice(2) : input).split('/');
	const parts: string[] = [];

	for (const segment of segments) {
		if (!segment || segment === '.') continue;
		if (segment === '..') {
			if (parts.length > 0 && parts.at(-1) !== '..') parts.pop();
			else if (!drive && !rooted && !isUnc) parts.push(segment);
			continue;
		}
		parts.push(segment.toLowerCase());
	}

	if (drive) return `${drive}/${parts.join('/')}`.replace(/\/$/, '/') || `${drive}/`;
	if (isUnc) return `//${parts.join('/')}`.replace(/\/$/, '/') || '//';
	if (rooted) return `/${parts.join('/')}`.replace(/\/$/, '/') || '/';
	return parts.join('/');
}

function isAbsoluteLegacyPath(path: string): boolean {
	return /^[a-zA-Z]:[\\/]/.test(path) || path.startsWith('\\\\') || path.startsWith('//') || path.startsWith('/');
}

function itemPhysicalPath(rootPath: string, relativePath: string): string {
	return normalizeLegacyPath(isAbsoluteLegacyPath(relativePath) ? relativePath : `${rootPath}/${relativePath}`);
}

function isSameOrDescendant(path: string, root: string): boolean {
	const normalizedPath = normalizeLegacyPath(path);
	const normalizedRoot = normalizeLegacyPath(root);
	return normalizedPath === normalizedRoot || normalizedPath.startsWith(`${normalizedRoot}/`);
}

/** Selects the shortest deterministic set of roots so one physical path is not imported twice. */
function legacyRecordKey(record: LegacyOrganizeStateSummary | LegacyOrganizeState): string {
	return 'key' in record ? record.key : record.directoryPath;
}

export function selectNonOverlappingLegacyStates<T extends LegacyOrganizeStateSummary | LegacyOrganizeState>(records: T[]): T[] {
	const selected: T[] = [];
	const ordered = [...records].sort((left, right) => {
		const leftPath = normalizeLegacyPath(left.directoryPath);
		const rightPath = normalizeLegacyPath(right.directoryPath);
		return leftPath.length - rightPath.length || leftPath.localeCompare(rightPath) || legacyRecordKey(left).localeCompare(legacyRecordKey(right));
	});

	for (const record of ordered) {
		if (!selected.some((parent) => isSameOrDescendant(record.directoryPath, parent.directoryPath))) selected.push(record);
	}

	return selected;
}

function resultFor(key: string): LegacyImportResult {
	return {
		key,
		taskId: null,
		appliedItemIds: [],
		missingPaths: [],
		unsupportedDecisions: [],
		archived: false,
		failure: null,
	};
}

function errorMessage(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

function failure(key: string, path: string, error: unknown): LegacyImportFailure {
	return {key, path, message: errorMessage(error)};
}

function isCreateSuccess(outcome: OrganizeCreateOutcome): outcome is Extract<OrganizeCreateOutcome, {Created: unknown}> {
	return 'Created' in outcome;
}

function isSupportedDecision(decision: string | null | undefined): decision is 'keep' | 'discard' {
	return decision === 'keep' || decision === 'discard';
}

function decisionInput(decision: 'keep' | 'discard'): 'Keep' | 'Discard' {
	return decision === 'keep' ? 'Keep' : 'Discard';
}

function revisionAfterDecision(outcome: OrganizeDecisionOutcome): number {
	if ('Applied' in outcome) return outcome.Applied.revision;
	if ('InheritedNoOp' in outcome) return outcome.InheritedNoOp.revision;
	throw new Error(`legacy decision was not applied: ${JSON.stringify(outcome)}`);
}

async function scanDescendants(api: LegacyImportApi, taskId: string, rootItemId: string): Promise<Model[]> {
	const queue = [rootItemId];
	const items: Model[] = [];

	while (queue.length > 0) {
		const parentItemId = queue.shift()!;
		let cursor: string | null = null;

		do {
			const input: OrganizeChildrenInput = {
				task_id: taskId,
				parent_item_id: parentItemId,
				cursor,
				limit: CHILD_PAGE_SIZE,
				sort: 'Name',
				direction: 'Asc',
				filter: 'All',
			};
			const page = await api.listChildren(input);
			items.push(...page.items);
			for (const item of page.items) {
				if (item.kind.toLowerCase() === 'directory') queue.push(item.uuid);
			}
			cursor = page.next_cursor;
		} while (cursor !== null);
	}

	return items;
}

/** Imports one legacy record while keeping its source file until every required step succeeds. */
export async function importLegacyState(record: LegacyOrganizeRecord, api: LegacyImportApi): Promise<LegacyImportResult> {
	const result = resultFor(record.key);
	let currentPath = record.directoryPath;

	try {
		const createInput: OrganizeCreateInput = {
			root: {Physical: {device_slug: 'local', path: record.directoryPath}} as SdPath,
			name: null,
		};
		const created = await api.createTask(createInput);
		if (!isCreateSuccess(created)) {
			throw new Error(`organize task was not created: ${JSON.stringify(created)}`);
		}

		result.taskId = created.Created.task_id;
		await api.waitForJob(created.Created.snapshot_job.id);
		const task = await api.getTask({task_id: result.taskId} satisfies OrganizeGetInput);
		let revision = task.task.revision;
		const scannedItems = await scanDescendants(api, result.taskId, task.root_item_id);
		const byPath = new Map<string, Model>();
		for (const item of scannedItems.sort((left, right) => left.uuid.localeCompare(right.uuid))) {
			byPath.set(itemPhysicalPath(record.directoryPath, item.relative_path), item);
		}

		for (const legacyItem of Object.values(record.items)) {
			const decision = legacyItem.decision;
			if (!decision) continue;
			if (!isSupportedDecision(decision)) {
				result.unsupportedDecisions.push({path: legacyItem.path, decision});
				continue;
			}

			currentPath = legacyItem.path;
			const item = byPath.get(normalizeLegacyPath(legacyItem.path));
			if (!item) {
				result.missingPaths.push(legacyItem.path);
				continue;
			}

			const input: OrganizeSetDecisionInput = {
				task_id: result.taskId,
				selection: {Items: {item_ids: [item.uuid]}},
				decision: decisionInput(decision),
				expected_revision: revision,
				confirm_descendant_override: false,
				confirm_ancestor_split: false,
			};
			revision = revisionAfterDecision(await api.setDecision(input));
			result.appliedItemIds.push(item.uuid);
		}

		if (result.unsupportedDecisions.length > 0) return result;
		currentPath = record.directoryPath;
		await api.archiveLegacyState(record.key);
		result.archived = true;
		return result;
	} catch (error) {
		result.failure = failure(record.key, currentPath, error);
		return result;
	}
}

/** Imports records in deterministic non-overlapping root order. */
export async function importLegacyStates(records: LegacyOrganizeRecord[], api: LegacyImportApi): Promise<LegacyImportResult[]> {
	const selected = selectNonOverlappingLegacyStates(records);
	const results: LegacyImportResult[] = [];
	for (const record of selected) results.push(await importLegacyState(record, api));
	return results;
}

/** Connects the pure importer to the current Tauri platform and library client. */
export function createLegacyImportApi(client: {execute<I, O>(wireMethod: string, input: I): Promise<O>}, platform: Pick<Platform, 'archiveLegacyOrganizeState'>): LegacyImportApi {
	return {
		createTask: (input) => client.execute<OrganizeCreateInput, OrganizeCreateOutcome>('action:organize.create.input', input),
		waitForJob: async (jobId) => {
			const deadline = Date.now() + SNAPSHOT_TIMEOUT_MS;
			while (Date.now() < deadline) {
				const info = await client.execute<{job_id: string}, JobInfoOutput>('query:jobs.info', {job_id: jobId});
				if (info.status === 'completed') return;
				if (info.status === 'failed' || info.status === 'cancelled') throw new Error(info.error_message ?? `snapshot job ${jobId} ${info.status}`);
				await new Promise((resolve) => setTimeout(resolve, SNAPSHOT_POLL_MS));
			}
			throw new Error(`snapshot job ${jobId} timed out`);
		},
		getTask: (input) => client.execute<OrganizeGetInput, OrganizeGetOutput>('query:organize.get', input),
		listChildren: (input) => client.execute<OrganizeChildrenInput, import('@sd/ts-client').OrganizeChildrenOutput>('query:organize.children', input),
		setDecision: (input) => client.execute<OrganizeSetDecisionInput, OrganizeDecisionOutcome>('action:organize.set_decision.input', input),
		archiveLegacyState: async (key) => {
			if (!platform.archiveLegacyOrganizeState) throw new Error('legacy organize archive is unavailable on this platform');
			await platform.archiveLegacyOrganizeState(key);
		},
	};
}
