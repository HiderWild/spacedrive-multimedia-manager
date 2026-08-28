import type {
	OrganizeChildrenInput,
	OrganizeChildrenOutput,
	OrganizeCreateInput,
	OrganizeCreateOutcome,
	OrganizeDecisionOutcome,
	OrganizeGetInput,
	OrganizeGetOutput,
	OrganizeSetDecisionInput,
} from '@sd/ts-client';

export interface LegacyOrganizeItem {
	itemId: string | null;
	path: string;
	name: string;
	kind: string;
	decision?: string | null;
	updatedAt: string;
}

export interface LegacyOrganizeState {
	version: number;
	directoryPath: string;
	updatedAt: string;
	items: Record<string, LegacyOrganizeItem>;
}

export type LegacyOrganizeRecord = LegacyOrganizeState & {key: string};

export interface LegacyOrganizeStateSummary {
	key: string;
	version: number;
	directoryPath: string;
	updatedAt: string;
	itemCount: number;
}

export interface LegacyImportFailure {
	key: string;
	path: string;
	message: string;
}

export interface LegacyUnsupportedDecision {
	path: string;
	decision: string;
}

export interface LegacyImportResult {
	key: string;
	taskId: string | null;
	appliedItemIds: string[];
	missingPaths: string[];
	unsupportedDecisions: LegacyUnsupportedDecision[];
	archived: boolean;
	failure: LegacyImportFailure | null;
}

export interface LegacyImportApi {
	createTask(input: OrganizeCreateInput): Promise<OrganizeCreateOutcome>;
	waitForJob(jobId: string): Promise<void>;
	getTask(input: OrganizeGetInput): Promise<OrganizeGetOutput>;
	listChildren(input: OrganizeChildrenInput): Promise<OrganizeChildrenOutput>;
	setDecision(input: OrganizeSetDecisionInput): Promise<OrganizeDecisionOutcome>;
	archiveLegacyState(key: string): Promise<void>;
}
