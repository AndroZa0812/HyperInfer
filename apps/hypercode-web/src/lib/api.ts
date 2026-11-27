import type {
	GitDiffResponse,
	SessionDetail,
	SessionSummary,
	WebInfo,
	WebStreamEvent
} from './types';

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
	const res = await fetch(path, {
		...init,
		credentials: 'include',
		headers: {
			'Content-Type': 'application/json',
			...(init?.headers ?? {})
		}
	});
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(`${res.status}: ${text}`);
	}
	return (await res.json()) as T;
}

export const webApi = {
	authenticate: (token: string) =>
		fetchJson<{ authenticated: boolean }>('/api/auth', {
			method: 'POST',
			body: JSON.stringify({ token })
		}),

	getInfo: () => fetchJson<WebInfo>('/api/info'),

	listSessions: () => fetchJson<SessionSummary[]>('/api/sessions'),

	createSession: (title?: string) =>
		fetchJson<SessionSummary>('/api/sessions', {
			method: 'POST',
			body: JSON.stringify({ title })
		}),

	getSession: (id: string) =>
		fetchJson<SessionDetail>(`/api/sessions/${encodeURIComponent(id)}`),

	cancelSession: (id: string) =>
		fetchJson<{ cancelled: boolean }>(`/api/sessions/${encodeURIComponent(id)}/cancel`, {
			method: 'POST'
		}),

	resolveApproval: (approvalId: string, approved: boolean) =>
		fetchJson<{ resolved: boolean; approved: boolean }>(
			`/api/approvals/${encodeURIComponent(approvalId)}`,
			{
				method: 'POST',
				body: JSON.stringify({ approved })
			}
		),

	getGitDiff: () => fetchJson<GitDiffResponse>('/api/git/diff'),

	async streamMessage(
		sessionId: string,
		prompt: string,
		onEvent: (ev: WebStreamEvent) => void
	): Promise<void> {
		const res = await fetch(`/api/sessions/${encodeURIComponent(sessionId)}/messages`, {
			method: 'POST',
			credentials: 'include',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ prompt })
		});
		if (!res.ok || !res.body) {
			const text = await res.text().catch(() => res.statusText);
			throw new Error(`${res.status}: ${text}`);
		}

		const reader = res.body.getReader();
		const decoder = new TextDecoder();
		let buffer = '';
		let finished = false;

		while (!finished) {
			const { value, done } = await reader.read();
			if (done) break;
			buffer += decoder.decode(value, { stream: true });

			let sepIdx: number;
			while ((sepIdx = buffer.indexOf('\n\n')) !== -1) {
				const frame = buffer.slice(0, sepIdx);
				buffer = buffer.slice(sepIdx + 2);
				for (const line of frame.split('\n')) {
					if (line.startsWith('data:')) {
						const payload = line.slice(5).trim();
						if (!payload) continue;
						try {
							const ev = JSON.parse(payload) as WebStreamEvent;
							onEvent(ev);
							if (ev.type === 'turn_completed' || ev.type === 'error') {
								finished = true;
								break;
							}
						} catch {
							// Ignore non-JSON keepalive frames
						}
					}
				}
				if (finished) break;
			}
		}
		if (finished) {
			await reader.cancel().catch(() => {});
		}
	}
};
