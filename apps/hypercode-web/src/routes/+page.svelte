<script lang="ts">
	import { onMount } from 'svelte';
	import { webApi } from '#lib/api';
	import type {
		GitDiffResponse,
		PendingApprovalInfo,
		SessionDetail,
		SessionSummary,
		SubagentStatusEntry,
		WebChatMessage,
		WebInfo,
		WebToolExecution
	} from '#lib/types';
	import Button from '@hyperinfer/ui/Button.svelte';
	import Card from '@hyperinfer/ui/Card.svelte';
	import Input from '@hyperinfer/ui/Input.svelte';
	import Badge from '@hyperinfer/ui/Badge.svelte';

	let theme = $state<'light' | 'dark'>('dark');
	let authenticated = $state(false);
	let checkingAuth = $state(true);
	let tokenInput = $state('');
	let authError = $state('');
	let authenticating = $state(false);

	let info = $state<WebInfo | null>(null);
	let sessions = $state<SessionSummary[]>([]);
	let activeSessionId = $state<string | null>(null);
	let messages = $state<WebChatMessage[]>([]);
	let commandLogs = $state<WebToolExecution[]>([]);
	let subagents = $state<SubagentStatusEntry[]>([]);
	let pendingApprovals = $state<PendingApprovalInfo[]>([]);

	let promptText = $state('');
	let streaming = $state(false);
	let streamingDelta = $state('');
	let currentStep = $state(0);
	let lastUsage = $state<{ input: number; output: number; steps: number } | null>(null);
	let streamError = $state('');

	let rightTab = $state<'diffs' | 'output' | 'subagents'>('diffs');
	let gitDiff = $state<GitDiffResponse>({ branch: null, files: [], diff: '' });

	function applyTheme(next: 'light' | 'dark') {
		theme = next;
		if (typeof document !== 'undefined') {
			document.documentElement.setAttribute('data-theme', next);
			try {
				localStorage.setItem('hypercode-web-theme', next);
			} catch {
				// ignore storage errors
			}
		}
	}

	function toggleTheme() {
		applyTheme(theme === 'dark' ? 'light' : 'dark');
	}

	onMount(async () => {
		try {
			const saved = localStorage.getItem('hypercode-web-theme');
			if (saved === 'light' || saved === 'dark') {
				applyTheme(saved);
			} else {
				applyTheme('dark');
			}
		} catch {
			applyTheme('dark');
		}

		// Auto-consume ?token=... from URL and strip it from browser history
		const url = new URL(window.location.href);
		const urlToken = url.searchParams.get('token');
		if (urlToken) {
			url.searchParams.delete('token');
			window.history.replaceState({}, '', url.pathname + url.search + url.hash);
			try {
				await webApi.authenticate(urlToken);
				authenticated = true;
			} catch {
				authError = 'Token from URL was rejected.';
			}
		}

		if (!authenticated) {
			try {
				info = await webApi.getInfo();
				authenticated = true;
			} catch {
				authenticated = false;
			}
		}

		checkingAuth = false;
		if (authenticated) {
			await bootstrapWorkspace();
		}
	});

	async function handleTokenLogin(e: Event) {
		e.preventDefault();
		if (!tokenInput.trim()) {
			authError = 'Please enter the auth token printed by hypercode web.';
			return;
		}
		authenticating = true;
		authError = '';
		try {
			await webApi.authenticate(tokenInput.trim());
			authenticated = true;
			tokenInput = '';
			await bootstrapWorkspace();
		} catch {
			authError = 'Invalid auth token.';
		} finally {
			authenticating = false;
		}
	}

	async function bootstrapWorkspace() {
		try {
			const [infoRes, listRes, diffRes] = await Promise.all([
				webApi.getInfo(),
				webApi.listSessions(),
				webApi.getGitDiff()
			]);
			info = infoRes;
			sessions = listRes;
			gitDiff = diffRes;

			if (sessions.length === 0) {
				await createNewSession();
			} else {
				await selectSession(sessions[0].id);
			}
		} catch (e) {
			console.error('Failed to initialize workspace', e);
		}
	}

	async function createNewSession() {
		const created = await webApi.createSession();
		sessions = [created, ...sessions];
		await selectSession(created.id);
	}

	async function selectSession(id: string) {
		activeSessionId = id;
		streamError = '';
		try {
			const detail: SessionDetail = await webApi.getSession(id);
			messages = detail.messages;
			commandLogs = detail.command_logs;
			subagents = detail.subagents;
			pendingApprovals = detail.pending_approvals;
		} catch (e) {
			console.error('Failed to load session', e);
		}
	}

	async function refreshGitDiff() {
		try {
			gitDiff = await webApi.getGitDiff();
		} catch {
			// ignore diff errors on non-git dirs
		}
	}

	async function sendPrompt(e?: Event) {
		if (e) e.preventDefault();
		const text = promptText.trim();
		if (!text || streaming) return;

		if (!activeSessionId) {
			await createNewSession();
		}
		const sid = activeSessionId!;

		promptText = '';
		streamError = '';
		streaming = true;
		streamingDelta = '';
		currentStep = 0;

		messages = [
			...messages,
			{
				id: `local-${Date.now()}`,
				role: 'user',
				content: text,
				timestamp: new Date().toISOString()
			}
		];

		try {
			await webApi.streamMessage(sid, text, (ev) => {
				switch (ev.type) {
					case 'step_started':
						currentStep = ev.step;
						break;
					case 'assistant_delta':
						streamingDelta += ev.text;
						break;
					case 'tool_approval_required':
						pendingApprovals = [
							...pendingApprovals,
							{
								approval_id: ev.approval_id,
								session_id: sid,
								tool_name: ev.tool_name,
								preview: ev.preview,
								reason: ev.reason
							}
						];
						break;
					case 'tool_approval_resolved':
						pendingApprovals = pendingApprovals.filter(
							(p) => p.approval_id !== ev.approval_id
						);
						break;
					case 'tool_executed': {
						const exec: WebToolExecution = {
							tool_name: ev.tool_name,
							arguments: ev.arguments,
							output: ev.output,
							is_error: ev.is_error,
							timestamp: new Date().toISOString()
						};
						commandLogs = [...commandLogs, exec];
						refreshGitDiff();
						break;
					}
					case 'subagent_update': {
						const idx = subagents.findIndex((s) => s.id === ev.id);
						const updated: SubagentStatusEntry = {
							id: ev.id,
							role: ev.role,
							status: ev.status,
							summary: ev.summary,
							updated_at: new Date().toISOString()
						};
						if (idx === -1) {
							subagents = [...subagents, updated];
						} else {
							subagents = subagents.map((s, i) => (i === idx ? updated : s));
						}
						break;
					}
					case 'turn_completed':
						streaming = false;
						messages = [
							...messages,
							{
								id: `asst-${Date.now()}`,
								role: 'assistant',
								content: ev.final_response,
								timestamp: new Date().toISOString()
							}
						];
						streamingDelta = '';
						lastUsage = {
							input: ev.input_tokens,
							output: ev.output_tokens,
							steps: ev.steps_taken
						};
						refreshGitDiff();
						webApi.listSessions().then((list) => {
							sessions = list;
						});
						break;
					case 'error':
						streaming = false;
						streamError = ev.message;
						break;
				}
			});
		} catch (err) {
			streamError = err instanceof Error ? err.message : 'Streaming failed';
		} finally {
			streaming = false;
			pendingApprovals = [];
		}
	}

	async function decideApproval(approvalId: string, approved: boolean) {
		try {
			await webApi.resolveApproval(approvalId, approved);
			pendingApprovals = pendingApprovals.filter((p) => p.approval_id !== approvalId);
		} catch (e) {
			console.error('Failed to resolve approval', e);
		}
	}

	async function cancelTurn() {
		if (!activeSessionId) return;
		try {
			await webApi.cancelSession(activeSessionId);
		} catch (e) {
			console.error('Failed to cancel turn', e);
		}
	}

	function diffLineClass(line: string): string {
		if (line.startsWith('+++') || line.startsWith('---'))
			return 'text-[var(--on-surface-variant)] font-semibold';
		if (line.startsWith('@@')) return 'text-[var(--primary)] bg-[var(--primary)]/10';
		if (line.startsWith('+')) return 'text-[var(--success)] bg-[var(--success)]/10';
		if (line.startsWith('-')) return 'text-[var(--error)] bg-[var(--error)]/10';
		return 'text-[var(--on-surface-variant)]';
	}
</script>

{#if checkingAuth}
	<div class="min-h-screen flex items-center justify-center">
		<p class="text-sm text-[var(--on-surface-variant)]">Connecting to HyperCode daemon...</p>
	</div>
{:else if !authenticated}
	<div class="min-h-screen flex items-center justify-center p-6">
		<div class="w-full max-w-md">
			<Card variant="elevated">
				<div class="space-y-5">
					<div>
						<h1 class="text-2xl font-bold text-[var(--on-surface)]">HyperCode Web</h1>
						<p class="text-sm text-[var(--on-surface-variant)] mt-1">
							Enter the local daemon auth token printed by <code class="font-mono">hypercode web</code>.
						</p>
					</div>
					<form onsubmit={handleTokenLogin} class="space-y-4">
						<Input
							label="Auth Token"
							type="password"
							bind:value={tokenInput}
							placeholder="Paste token from terminal"
							error={authError}
						/>
						<Button type="submit" disabled={authenticating} class="w-full">
							{authenticating ? 'Authenticating...' : 'Connect'}
						</Button>
					</form>
				</div>
			</Card>
		</div>
	</div>
{:else}
	<div class="flex h-screen overflow-hidden">
		<!-- Left Sidebar: Sessions & Workspace Info -->
		<aside class="w-64 shrink-0 flex flex-col border-r border-[var(--ghost-border)] bg-[var(--surface-container-low)]">
			<div class="p-4 border-b border-[var(--ghost-border)] space-y-3">
				<div class="flex items-center justify-between">
					<div class="flex items-center gap-2">
						<span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 20px">terminal</span>
						<span class="font-bold text-sm text-[var(--on-surface)]">HyperCode</span>
					</div>
					<button
						type="button"
						aria-label="Toggle theme"
						class="p-1.5 rounded-lg text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] cursor-pointer"
						onclick={toggleTheme}
					>
						<span class="material-symbols-outlined" style="font-size: 18px">
							{theme === 'dark' ? 'light_mode' : 'dark_mode'}
						</span>
					</button>
				</div>

				<Button size="sm" class="w-full" onclick={createNewSession}>
					New Session
				</Button>
			</div>

			<div class="flex-1 overflow-y-auto p-2 space-y-1">
				{#each sessions as s}
					<button
						type="button"
						onclick={() => selectSession(s.id)}
						class="w-full text-left px-3 py-2 rounded-lg text-xs transition-colors cursor-pointer {activeSessionId === s.id
							? 'bg-[var(--primary)]/15 text-[var(--primary)] font-medium'
							: 'text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)]'}"
					>
						<p class="truncate">{s.title}</p>
						<p class="text-[10px] opacity-70 mt-0.5">{s.message_count} messages</p>
					</button>
				{/each}
			</div>

			{#if info}
				<div class="p-3 border-t border-[var(--ghost-border)] space-y-1.5 text-[11px]">
					<p class="font-mono text-[var(--on-surface)] truncate" title={info.workspace}>
						{info.workspace}
					</p>
					<div class="flex flex-wrap gap-1">
						<Badge variant="info" label={info.model} />
						{#if info.git_branch}
							<Badge variant="neutral" label={info.git_branch} />
						{/if}
						<Badge variant="warning" label={info.approval_mode} />
					</div>
				</div>
			{/if}
		</aside>

		<!-- Center Pane: Streaming Chat & Tool Approvals -->
		<main class="flex-1 flex flex-col min-w-0">
			<header class="px-6 py-3 border-b border-[var(--ghost-border)] flex items-center justify-between bg-[var(--surface-container-lowest)]">
				<div class="flex items-center gap-3">
					<h1 class="text-sm font-semibold text-[var(--on-surface)]">Interactive Agent</h1>
					{#if streaming}
						<Badge variant="info" label={`Running (step ${currentStep || 1})`} />
					{:else if lastUsage}
						<span class="text-xs font-mono text-[var(--on-surface-variant)]">
							{lastUsage.steps} steps · {lastUsage.input} in / {lastUsage.output} out
						</span>
					{/if}
				</div>
				{#if info}
					<span class="text-xs text-[var(--on-surface-variant)]">{info.sandbox}</span>
				{/if}
			</header>

			<div class="flex-1 overflow-y-auto p-6 space-y-4">
				{#if messages.length === 0 && !streaming}
					<div class="h-full flex items-center justify-center">
						<p class="text-sm text-[var(--on-surface-variant)]">
							Ask HyperCode to inspect, edit, or test files in your local workspace.
						</p>
					</div>
				{/if}

				{#each messages as msg}
					<div class="flex {msg.role === 'user' ? 'justify-end' : 'justify-start'}">
						<div
							class="max-w-2xl rounded-xl px-4 py-3 text-sm {msg.role === 'user'
								? 'bg-[var(--primary)]/15 text-[var(--on-surface)]'
								: 'bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] text-[var(--on-surface)]'}"
						>
							<p class="text-[10px] uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
								{msg.role}
							</p>
							<div class="whitespace-pre-wrap break-words font-sans">{msg.content}</div>
						</div>
					</div>
				{/each}

				{#if streamingDelta}
					<div class="flex justify-start">
						<div class="max-w-2xl rounded-xl px-4 py-3 text-sm bg-[var(--surface-container-lowest)] border border-[var(--primary)]/40 text-[var(--on-surface)]">
							<p class="text-[10px] uppercase tracking-wider text-[var(--primary)] mb-1">
								assistant (streaming)
							</p>
							<div class="whitespace-pre-wrap break-words">{streamingDelta}</div>
						</div>
					</div>
				{/if}

				{#each pendingApprovals as req}
					<Card variant="outlined" padding="sm" class="border-[var(--warning)]">
						<div class="space-y-3">
							<div class="flex items-center justify-between gap-2">
								<div class="flex items-center gap-2">
									<Badge variant="warning" label="Approval Required" />
									<span class="font-mono text-xs font-semibold text-[var(--on-surface)]">
										{req.tool_name}
									</span>
								</div>
								<span class="text-xs text-[var(--on-surface-variant)]">{req.reason}</span>
							</div>
							<pre class="p-2.5 rounded-lg bg-[var(--surface-container-low)] text-xs font-mono text-[var(--on-surface)] overflow-x-auto whitespace-pre-wrap">{req.preview}</pre>
							<div class="flex justify-end gap-2">
								<Button
									variant="secondary"
									size="sm"
									onclick={() => decideApproval(req.approval_id, false)}
								>
									Deny
								</Button>
								<Button
									size="sm"
									onclick={() => decideApproval(req.approval_id, true)}
								>
									Approve
								</Button>
							</div>
						</div>
					</Card>
				{/each}

				{#if streamError}
					<div class="p-3 rounded-lg bg-[var(--error-container)]/30 text-xs text-[var(--error)]">
						{streamError}
					</div>
				{/if}
			</div>

			<form
				onsubmit={sendPrompt}
				class="p-4 border-t border-[var(--ghost-border)] bg-[var(--surface-container-lowest)] flex gap-2 items-end"
			>
				<textarea
					bind:value={promptText}
					rows="2"
					placeholder="Describe a task for HyperCode..."
					disabled={streaming}
					onkeydown={(e) => {
						if (e.key === 'Enter' && !e.shiftKey) {
							e.preventDefault();
							sendPrompt();
						}
					}}
					class="flex-1 px-4 py-2.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm resize-none"
				></textarea>
				{#if streaming}
					<Button variant="secondary" onclick={cancelTurn}>Cancel</Button>
				{:else}
					<Button type="submit" disabled={!promptText.trim()}>Send</Button>
				{/if}
			</form>
		</main>

		<!-- Right Inspector Drawer: Live Diffs, Command Output, Subagents -->
		<aside class="w-96 shrink-0 flex flex-col border-l border-[var(--ghost-border)] bg-[var(--surface-container-low)]">
			<div class="flex border-b border-[var(--ghost-border)] text-xs font-medium">
				<button
					type="button"
					onclick={() => {
						rightTab = 'diffs';
						refreshGitDiff();
					}}
					class="flex-1 py-3 text-center cursor-pointer border-b-2 transition-colors {rightTab === 'diffs'
						? 'border-[var(--primary)] text-[var(--primary)]'
						: 'border-transparent text-[var(--on-surface-variant)]'}"
				>
					Live Diffs ({gitDiff.files.length})
				</button>
				<button
					type="button"
					onclick={() => (rightTab = 'output')}
					class="flex-1 py-3 text-center cursor-pointer border-b-2 transition-colors {rightTab === 'output'
						? 'border-[var(--primary)] text-[var(--primary)]'
						: 'border-transparent text-[var(--on-surface-variant)]'}"
				>
					Output ({commandLogs.length})
				</button>
				<button
					type="button"
					onclick={() => (rightTab = 'subagents')}
					class="flex-1 py-3 text-center cursor-pointer border-b-2 transition-colors {rightTab === 'subagents'
						? 'border-[var(--primary)] text-[var(--primary)]'
						: 'border-transparent text-[var(--on-surface-variant)]'}"
				>
					Subagents ({subagents.length})
				</button>
			</div>

			<div class="flex-1 overflow-y-auto p-3 space-y-3">
				{#if rightTab === 'diffs'}
					<div class="flex items-center justify-between">
						<span class="text-xs font-medium text-[var(--on-surface-variant)]">
							Workspace Changes
						</span>
						<Button variant="ghost" size="sm" onclick={refreshGitDiff}>Refresh</Button>
					</div>

					{#if gitDiff.files.length === 0 && !gitDiff.diff}
						<p class="text-xs text-[var(--on-surface-variant)] py-6 text-center">
							Working tree is clean.
						</p>
					{:else}
						<div class="space-y-1">
							{#each gitDiff.files as f}
								<div class="flex items-center justify-between px-2.5 py-1.5 rounded bg-[var(--surface-container-lowest)] text-xs font-mono">
									<span class="truncate text-[var(--on-surface)]">{f.path}</span>
									<Badge variant="neutral" label={f.status} />
								</div>
							{/each}
						</div>
						{#if gitDiff.diff}
							<div class="rounded-lg bg-[var(--surface-container-lowest)] p-2 overflow-x-auto font-mono text-[11px] leading-5">
								{#each gitDiff.diff.split('\n') as line}
									<div class="px-1 whitespace-pre {diffLineClass(line)}">{line}</div>
								{/each}
							</div>
						{/if}
					{/if}
				{:else if rightTab === 'output'}
					{#if commandLogs.length === 0}
						<p class="text-xs text-[var(--on-surface-variant)] py-6 text-center">
							No tool executions yet.
						</p>
					{:else}
						{#each commandLogs as log}
							<Card variant="outlined" padding="sm">
								<div class="space-y-1.5">
									<div class="flex items-center justify-between gap-2">
										<span class="font-mono text-xs font-semibold text-[var(--on-surface)]">
											{log.tool_name}
										</span>
										<Badge
											variant={log.is_error ? 'error' : 'success'}
											label={log.is_error ? 'error' : 'ok'}
										/>
									</div>
									<pre class="text-[11px] font-mono text-[var(--on-surface-variant)] bg-[var(--surface-container-low)] p-2 rounded overflow-x-auto whitespace-pre-wrap">{log.output}</pre>
								</div>
							</Card>
						{/each}
					{/if}
				{:else if rightTab === 'subagents'}
					{#if subagents.length === 0}
						<p class="text-xs text-[var(--on-surface-variant)] py-6 text-center">
							No subagents spawned in this session.
						</p>
					{:else}
						{#each subagents as sa}
							<Card variant="outlined" padding="sm">
								<div class="space-y-1">
									<div class="flex items-center justify-between">
										<span class="text-xs font-semibold text-[var(--on-surface)]">{sa.role}</span>
										<Badge variant="info" label={sa.status} />
									</div>
									<p class="text-xs text-[var(--on-surface-variant)]">{sa.summary}</p>
								</div>
							</Card>
						{/each}
					{/if}
				{/if}
			</div>
		</aside>
	</div>
{/if}
