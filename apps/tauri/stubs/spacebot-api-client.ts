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
	portalSend: async () => {},
	createPortalConversation: async () => ({ conversation: { id: 'stub' } }),
	listWorkers: async () => ({ workers: [] }),
	updateWorker: async () => {},
	getWorkerStatus: async () => ({ status: 'unavailable' }),
	getTimeline: async () => ({ items: [] }),
	createTtsProfile: async () => {},
	synthesizeSpeech: async () => new ArrayBuffer(0),
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
};

export type Task = {
	id: string;
	title: string;
	status: string;
	priority?: string;
	description?: string;
};

export type UpdateTaskRequest = {
	taskId: string;
	title?: string;
	status?: string;
	priority?: string;
};

export type TimelineItem = {
	type: string;
	id: string;
	role: string;
	content: string;
	sender_id?: string | null;
	sender_name?: string | null;
	created_at: string;
};

export type WorkerListItem = {
	id: string;
	name: string;
	status: string;
	detail?: string;
};

export type TtsProfile = {
	id: string;
	name: string;
	voice_id?: string;
};

export function mockSpacebotUnavailable(): never {
	throw new Error('Spacebot is not available in this build');
}