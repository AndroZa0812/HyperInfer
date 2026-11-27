export interface WebInfo {
	workspace: string;
	git_branch: string | null;
	model: string;
	approval_mode: string;
	sandbox: string;
	version: string;
}

export interface WebToolExecution {
	tool_name: string;
	arguments: string;
	output: string;
	is_error: boolean;
	timestamp: string;
}

export interface WebChatMessage {
	id: string;
	role: string;
	content: string;
	timestamp: string;
	tool_events?: WebToolExecution[];
}

export interface SubagentStatusEntry {
	id: string;
	role: string;
	status: string;
	summary: string;
	updated_at: string;
}

export interface PendingApprovalInfo {
	approval_id: string;
	session_id: string;
	tool_name: string;
	preview: string;
	reason: string;
}

export interface SessionSummary {
	id: string;
	title: string;
	created_at: string;
	message_count: number;
	running: boolean;
}

export interface SessionDetail {
	id: string;
	title: string;
	created_at: string;
	running: boolean;
	messages: WebChatMessage[];
	command_logs: WebToolExecution[];
	subagents: SubagentStatusEntry[];
	pending_approvals: PendingApprovalInfo[];
}

export interface GitFileStatus {
	path: string;
	status: string;
}

export interface GitDiffResponse {
	branch: string | null;
	files: GitFileStatus[];
	diff: string;
}

export type WebStreamEvent =
	| { type: 'turn_started'; session_id: string; prompt: string }
	| { type: 'step_started'; step: number }
	| { type: 'assistant_delta'; text: string }
	| {
			type: 'tool_approval_required';
			approval_id: string;
			tool_name: string;
			preview: string;
			reason: string;
	  }
	| {
			type: 'tool_approval_resolved';
			approval_id: string;
			tool_name: string;
			approved: boolean;
			reason: string;
	  }
	| {
			type: 'tool_executed';
			tool_name: string;
			arguments: string;
			output: string;
			is_error: boolean;
	  }
	| {
			type: 'subagent_update';
			id: string;
			role: string;
			status: string;
			summary: string;
	  }
	| { type: 'context_compacted'; tokens_before: number; tokens_after: number }
	| {
			type: 'turn_completed';
			final_response: string;
			steps_taken: number;
			input_tokens: number;
			output_tokens: number;
	  }
	| { type: 'error'; message: string };
