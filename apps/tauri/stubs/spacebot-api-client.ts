// Stub for @spacebot/api-client when spacebot is not available locally
// This module provides mock implementations so the UI can still load
// and render, with Spacebot features gracefully disabled.

export const apiClient = {
	chat: {
		completions: {
			create: async () => {
				throw new Error('Spacebot is not available');
			}
		}
	},
	audio: {
		speech: {
			create: async () => {
				throw new Error('Spacebot is not available');
			}
		}
	},
	listPortalConversations: async () => ({ conversations: [] }),
	portalHistory: async () => [],
	channelMessages: async () => ({ items: [], has_more: false }),
	portalSend: async () => {},
	createPortalConversation: async () => ({ conversation: { id: 'stub' } }),
	listWorkers: async () => ({ workers: [] }),
	workerDetail: async (_agentId: string, workerId: string) => ({
		id: workerId,
		task: 'Spacebot unavailable',
		status: 'unavailable',
		started_at: new Date(0).toISOString(),
		completed_at: new Date(0).toISOString(),
		result: null,
		transcript: []
	}),
	cancelProcess: async () => ({}),
	updateWorker: async () => {},
	getWorkerStatus: async () => ({ status: 'unavailable' }),
	getTimeline: async () => ({ items: [] }),
	listTasks: async () => ({ tasks: [] }),
	updateTask: async () => {},
	deleteTask: async () => {},
	createTtsProfile: async () => {},
	ttsProfiles: async () => [],
	ttsGenerate: async () => new ArrayBuffer(0),
	synthesizeSpeech: async () => new ArrayBuffer(0),
	webChatSendAudio: async () =>
		new Response(null, {
			status: 503,
			statusText: 'Spacebot is not available'
		}),
};

export function getEventsUrl(): string {
	return '';
}

export function setServerUrl(_url: string): void {
	// No-op in stub mode
}

// Types used by Spacebot UI components

export type ChatCompletion = {
	id: string;
	choices: Array<{
		message: {
			content: string | null;
			role: string;
		};
		finish_reason: string | null;
	}>;
};

export type ChatCompletionChunk = {
	id: string;
	choices: Array<{
		delta: {
			content?: string;
			role?: string;
		};
		finish_reason: string | null;
	}>;
};

export type InboundMessageEvent = {
	agent_id: string;
	channel_id: string;
	text: string;
	sender_id?: string;
	sender_name?: string;
};

export type OutboundMessageEvent = {
	agent_id: string;
	channel_id: string;
	text: string;
};

export type OutboundMessageDeltaEvent = {
	agent_id: string;
	channel_id: string;
	aggregated_text: string;
};

export type TypingStateEvent = {
	agent_id: string;
	channel_id: string;
	is_typing: boolean;
};

export type PortalConversationResponse = {
	conversation: {
		id: string;
		title?: string;
	};
};

export type PortalConversationSummary = {
	id: string;
	title?: string;
	created_at?: string;
	message_count?: number;
};

export type PortalHistoryMessage = {
	id?: string;
	role: 'user' | 'assistant';
	content: string;
	timestamp: string;
};

export type Task = {
	id: string;
	task_number?: number;
	title: string;
	status: string;
	priority?: string;
	description?: string;
	subtasks?: Array<{
		title: string;
		completed: boolean;
	}>;
	assignee?: string;
};

export type UpdateTaskRequest = {
	taskId: string;
	title?: string;
	status?: string;
	priority?: string;
	complete_subtask?: number;
};

export type TimelineItem = {
	type: string;
	id: string;
	role: string;
	content: string;
	task?: string;
	status?: string;
	started_at?: string;
	completed_at?: string | null;
	sender_id?: string | null;
	sender_name?: string | null;
	created_at: string;
};

export type WorkerListItem = {
	id: string;
	name?: string;
	task?: string;
	status: string;
	detail?: string;
	worker_type?: string;
	channel_id?: string | null;
	channel_name?: string | null;
	started_at?: string;
	completed_at?: string | null;
	has_transcript?: boolean;
	live_status?: string | null;
	tool_calls?: number;
	opencode_port?: number | null;
	opencode_session_id?: string | null;
	directory?: string | null;
	interactive?: boolean;
	project_id?: string | null;
	project_name?: string | null;
};

export type TtsProfile = {
	id: string;
	name: string;
	voice_id?: string;
};

export function mockSpacebotUnavailable(): never {
	throw new Error('Spacebot is not available in this build');
}
