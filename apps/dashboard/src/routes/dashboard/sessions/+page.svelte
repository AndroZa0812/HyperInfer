<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type {
        AgentSession,
        AgentSessionsFilter,
        ApiKey,
        Team,
        User,
    } from '#lib/types';
    import Card from '#lib/components/Card.svelte';
    import Button from '#lib/components/Button.svelte';
    import Badge from '#lib/components/Badge.svelte';
    import StatCard from '#lib/components/StatCard.svelte';
    import Modal from '#lib/components/Modal.svelte';

    let sessions = $state<AgentSession[]>([]);
    let total = $state(0);
    let limit = $state(25);
    let offset = $state(0);
    let loading = $state(true);
    let error = $state('');

    let teams = $state<Team[]>([]);
    let users = $state<User[]>([]);
    let keys = $state<ApiKey[]>([]);

    // Filter state
    let teamFilter = $state('');
    let keyFilter = $state('');
    let statusFilter = $state('');
    let modelFilter = $state('');

    // Trajectory inspector modal state (read-only)
    let showInspector = $state(false);
    let inspecting = $state(false);
    let inspectError = $state('');
    let selectedSession = $state<AgentSession | null>(null);

    let isAdmin = $derived($auth.user?.role === 'admin');

    let totalCostUsd = $derived(
        sessions.reduce((acc, s) => acc + (s.cost_cents || 0), 0) / 100
    );
    let totalCacheSavingsUsd = $derived(
        sessions.reduce((acc, s) => acc + (s.cache_savings_cents || 0), 0) / 100
    );
    let avgLatencyMs = $derived(
        sessions.length > 0
            ? Math.round(
                  sessions.reduce((acc, s) => acc + s.total_latency_ms, 0) /
                      sessions.length
              )
            : 0
    );
    let totalMcpCalls = $derived(
        sessions.reduce(
            (acc, s) =>
                acc +
                (s.mcp_tool_calls || []).reduce((sum, c) => sum + c.count, 0),
            0
        )
    );

    let initialized = false;

    $effect(() => {
        if ($auth.loading || !$auth.user || initialized) return;
        initialized = true;
        const teamScope = isAdmin ? undefined : $auth.user.team_id;
        Promise.allSettled([
            isAdmin ? api.getTeams() : Promise.resolve([]),
            isAdmin ? api.getUsers() : Promise.resolve([]),
            api.getKeys(teamScope),
        ]).then(([tRes, uRes, kRes]) => {
            if (tRes.status === 'fulfilled') teams = tRes.value;
            if (uRes.status === 'fulfilled') users = uRes.value;
            if (kRes.status === 'fulfilled') keys = kRes.value;
        });
        loadSessions();
    });

    function buildFilter(): AgentSessionsFilter {
        const f: AgentSessionsFilter = { limit, offset };
        if (isAdmin && teamFilter) f.team_id = teamFilter;
        if (keyFilter) f.api_key_id = keyFilter;
        if (statusFilter) f.status = statusFilter;
        if (modelFilter.trim()) f.model = modelFilter.trim();
        return f;
    }

    async function loadSessions() {
        loading = true;
        error = '';
        try {
            const page = await api.getAgentSessions(buildFilter());
            sessions = page.sessions;
            total = page.total;
        } catch (e) {
            console.error('Failed to load agent sessions', e);
            error = 'Failed to load HyperCode agent sessions.';
        } finally {
            loading = false;
        }
    }

    function applyFilters() {
        offset = 0;
        loadSessions();
    }

    function resetFilters() {
        teamFilter = '';
        keyFilter = '';
        statusFilter = '';
        modelFilter = '';
        offset = 0;
        loadSessions();
    }

    function prevPage() {
        offset = Math.max(0, offset - limit);
        loadSessions();
    }

    function nextPage() {
        if (offset + limit < total) {
            offset += limit;
            loadSessions();
        }
    }

    async function openInspector(session: AgentSession) {
        showInspector = true;
        inspecting = true;
        inspectError = '';
        selectedSession = session;
        try {
            selectedSession = await api.getAgentSession(session.id);
        } catch (e) {
            console.error('Failed to load trajectory', e);
            inspectError = 'Failed to load redacted trajectory details.';
        } finally {
            inspecting = false;
        }
    }

    function teamLabel(teamId: string): string {
        return teams.find((t) => t.id === teamId)?.name || teamId.slice(0, 8);
    }

    function userLabel(userId?: string | null): string {
        if (!userId) return '—';
        return users.find((u) => u.id === userId)?.email || userId.slice(0, 8);
    }

    function keyLabel(keyId?: string | null): string {
        if (!keyId) return '—';
        const k = keys.find((x) => x.id === keyId);
        return k?.name || keyId.slice(0, 8);
    }

    function statusVariant(
        status: string
    ): 'success' | 'info' | 'warning' | 'error' | 'neutral' {
        if (status === 'completed') return 'success';
        if (status === 'running') return 'info';
        if (status === 'cancelled') return 'warning';
        if (status === 'failed') return 'error';
        return 'neutral';
    }

    function eventKindVariant(
        kind: string
    ): 'success' | 'info' | 'warning' | 'error' | 'neutral' {
        if (kind === 'mcp_tool_call') return 'info';
        if (kind === 'error') return 'error';
        if (kind === 'assistant' || kind === 'tool_result') return 'success';
        return 'neutral';
    }

    function sessionMcpCount(s: AgentSession): number {
        return (s.mcp_tool_calls || []).reduce((sum, c) => sum + c.count, 0);
    }

    function formatUsd(dollars: number): string {
        if (dollars === 0) return '$0.00';
        if (dollars < 0.01) return `$${dollars.toFixed(4)}`;
        return `$${dollars.toFixed(3)}`;
    }
</script>

<div class="space-y-8">
    <div class="flex flex-wrap items-center justify-between gap-4">
        <div>
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">
                Agent Sessions
            </h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                Read-only fleet observability and redacted trajectory governance across HyperCode runs.
            </p>
        </div>
        <Button variant="secondary" size="sm" onclick={loadSessions}>
            <span class="material-symbols-outlined mr-1" style="font-size: 16px">refresh</span>
            Refresh
        </Button>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-4">
        <StatCard
            label="Matching Sessions"
            value={total.toLocaleString()}
            icon="smart_toy"
        />
        <StatCard
            label="Weighted Cost (Page)"
            value={formatUsd(totalCostUsd)}
            icon="payments"
        />
        <StatCard
            label="Cache Hit Savings"
            value={formatUsd(totalCacheSavingsUsd)}
            icon="savings"
        />
        <StatCard
            label="Avg Latency (Page)"
            value={`${avgLatencyMs} ms`}
            icon="speed"
        />
        <StatCard
            label="MCP Tool Calls"
            value={totalMcpCalls.toLocaleString()}
            icon="hub"
        />
    </div>

    <Card>
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 items-end">
            {#if isAdmin}
                <div>
                    <label
                        for="sessions-team-filter"
                        class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1"
                    >
                        Team
                    </label>
                    <select
                        id="sessions-team-filter"
                        bind:value={teamFilter}
                        class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                    >
                        <option value="">All Teams</option>
                        {#each teams as t}
                            <option value={t.id}>{t.name}</option>
                        {/each}
                    </select>
                </div>
            {/if}

            <div>
                <label
                    for="sessions-key-filter"
                    class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1"
                >
                    Virtual API Key
                </label>
                <select
                    id="sessions-key-filter"
                    bind:value={keyFilter}
                    class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                >
                    <option value="">All Keys</option>
                    {#each keys as k}
                        <option value={k.id}>{k.name || k.id.slice(0, 8)}</option>
                    {/each}
                </select>
            </div>

            <div>
                <label
                    for="sessions-status-filter"
                    class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1"
                >
                    Status
                </label>
                <select
                    id="sessions-status-filter"
                    bind:value={statusFilter}
                    class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                >
                    <option value="">All Statuses</option>
                    <option value="running">Running</option>
                    <option value="completed">Completed</option>
                    <option value="failed">Failed</option>
                    <option value="cancelled">Cancelled</option>
                </select>
            </div>

            <div>
                <label
                    for="sessions-model-filter"
                    class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1"
                >
                    Model
                </label>
                <input
                    id="sessions-model-filter"
                    type="text"
                    bind:value={modelFilter}
                    placeholder="e.g. gpt-4o, claude"
                    class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                />
            </div>
        </div>

        <div class="flex justify-end gap-2 mt-4">
            <Button variant="secondary" size="sm" onclick={resetFilters}>
                Reset
            </Button>
            <Button size="sm" onclick={applyFilters}>Apply Filters</Button>
        </div>
    </Card>

    {#if error}
        <div class="flex items-center gap-2 text-sm text-[var(--error)] bg-[var(--error-container)]/20 rounded-lg px-4 py-3">
            <span class="material-symbols-outlined" style="font-size: 18px">error</span>
            {error}
        </div>
    {/if}

    <Card>
        {#if loading}
            <p class="text-sm text-[var(--on-surface-variant)] py-8 text-center">
                Loading agent sessions...
            </p>
        {:else if sessions.length === 0}
            <p class="text-sm text-[var(--on-surface-variant)] py-8 text-center">
                No HyperCode agent sessions match the current filters.
            </p>
        {:else}
            <div class="overflow-x-auto">
                <table class="w-full text-left border-collapse text-sm">
                    <thead>
                        <tr class="border-b border-[var(--ghost-border)] text-xs uppercase tracking-wider text-[var(--on-surface-variant)]">
                            <th class="py-3 pr-4">Session</th>
                            <th class="py-3 pr-4">Attribution</th>
                            <th class="py-3 pr-4">Model</th>
                            <th class="py-3 pr-4">Status</th>
                            <th class="py-3 pr-4">Tokens</th>
                            <th class="py-3 pr-4">Cost / Saved</th>
                            <th class="py-3 pr-4">Latency</th>
                            <th class="py-3 pr-4">MCP Calls</th>
                            <th class="py-3 text-right">Trajectory</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-[var(--ghost-border)]">
                        {#each sessions as s}
                            <tr class="hover:bg-[var(--surface-container)]/40 transition-colors">
                                <td class="py-3 pr-4">
                                    <p class="font-mono text-xs font-semibold text-[var(--on-surface)]">
                                        {s.session_key}
                                    </p>
                                    {#if s.title}
                                        <p class="text-xs text-[var(--on-surface-variant)] truncate max-w-[260px]">
                                            {s.title}
                                        </p>
                                    {/if}
                                </td>
                                <td class="py-3 pr-4 text-xs">
                                    <p class="text-[var(--on-surface)] font-medium">
                                        {teamLabel(s.team_id)}
                                    </p>
                                    <p class="text-[var(--on-surface-variant)]">
                                        {userLabel(s.user_id)} · Key: {keyLabel(s.api_key_id)}
                                    </p>
                                </td>
                                <td class="py-3 pr-4 font-mono text-xs text-[var(--on-surface)]">
                                    {s.model}
                                </td>
                                <td class="py-3 pr-4">
                                    <Badge
                                        variant={statusVariant(s.status)}
                                        label={s.status}
                                    />
                                </td>
                                <td class="py-3 pr-4 text-xs font-mono text-[var(--on-surface)]">
                                    <div>
                                        {s.input_tokens.toLocaleString()} in / {s.output_tokens.toLocaleString()} out
                                    </div>
                                    {#if s.cached_input_tokens > 0 || s.reasoning_tokens > 0}
                                        <div class="text-[var(--on-surface-variant)]">
                                            {s.cached_input_tokens.toLocaleString()} cached · {s.reasoning_tokens.toLocaleString()} rsn
                                        </div>
                                    {/if}
                                </td>
                                <td class="py-3 pr-4 text-xs font-mono">
                                    <div class="text-[var(--on-surface)]">
                                        {formatUsd(s.cost_cents / 100)}
                                    </div>
                                    {#if s.cache_savings_cents > 0}
                                        <div class="text-[var(--success)]">
                                            -{formatUsd(s.cache_savings_cents / 100)} saved
                                        </div>
                                    {/if}
                                </td>
                                <td class="py-3 pr-4 text-xs font-mono text-[var(--on-surface)]">
                                    {s.total_latency_ms.toLocaleString()} ms
                                </td>
                                <td class="py-3 pr-4 text-xs font-mono text-[var(--on-surface)]">
                                    {sessionMcpCount(s)}
                                </td>
                                <td class="py-3 text-right">
                                    <Button
                                        variant="secondary"
                                        size="sm"
                                        onclick={() => openInspector(s)}
                                    >
                                        Inspect
                                    </Button>
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>

            <div class="flex items-center justify-between pt-4 mt-4 border-t border-[var(--ghost-border)] text-xs text-[var(--on-surface-variant)]">
                <span>
                    Showing {offset + 1}–{Math.min(offset + sessions.length, total)} of {total}
                </span>
                <div class="flex gap-2">
                    <Button
                        variant="secondary"
                        size="sm"
                        disabled={offset === 0}
                        onclick={prevPage}
                    >
                        Previous
                    </Button>
                    <Button
                        variant="secondary"
                        size="sm"
                        disabled={offset + limit >= total}
                        onclick={nextPage}
                    >
                        Next
                    </Button>
                </div>
            </div>
        {/if}
    </Card>
</div>

<Modal
    bind:open={showInspector}
    onclose={() => {
        showInspector = false;
    }}
>
    <div class="p-6 space-y-5">
        <div class="flex items-center justify-between gap-4 border-b border-[var(--ghost-border)] pb-3">
            <h2 class="text-lg font-semibold text-[var(--on-surface)]">
                {selectedSession ? `Trajectory: ${selectedSession.session_key}` : 'Session Trajectory'}
            </h2>
            <button
                type="button"
                aria-label="Close modal"
                class="text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer"
                onclick={() => { showInspector = false; }}
            >
                <span class="material-symbols-outlined" style="font-size: 20px">close</span>
            </button>
        </div>
    {#if inspecting}
        <p class="text-sm text-[var(--on-surface-variant)] py-6 text-center">
            Loading redacted trajectory...
        </p>
    {:else if inspectError}
        <p class="text-sm text-[var(--error)] py-4">{inspectError}</p>
    {:else if selectedSession}
        <div class="space-y-5 max-h-[70vh] overflow-y-auto pr-1">
            <div class="flex items-center gap-2 text-xs text-[var(--on-surface-variant)] bg-[var(--surface-container-low)] rounded-lg px-3 py-2">
                <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 16px">shield</span>
                <span>
                    Read-only redacted trajectory view (PII &amp; secrets scrubbed). Agent tools never execute on the gateway server.
                </span>
            </div>

            {#if selectedSession.title}
                <div>
                    <p class="text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">
                        Task Summary (Redacted)
                    </p>
                    <p class="text-sm text-[var(--on-surface)] mt-1">
                        {selectedSession.title}
                    </p>
                </div>
            {/if}

            <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
                <div class="p-2.5 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Weighted Cost</p>
                    <p class="font-mono font-semibold text-[var(--on-surface)] mt-0.5">
                        {formatUsd(selectedSession.cost_cents / 100)}
                    </p>
                </div>
                <div class="p-2.5 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Cache Savings</p>
                    <p class="font-mono font-semibold text-[var(--success)] mt-0.5">
                        {formatUsd(selectedSession.cache_savings_cents / 100)}
                    </p>
                </div>
                <div class="p-2.5 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Latency</p>
                    <p class="font-mono font-semibold text-[var(--on-surface)] mt-0.5">
                        {selectedSession.total_latency_ms.toLocaleString()} ms
                    </p>
                </div>
                <div class="p-2.5 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Cache Hits</p>
                    <p class="font-mono font-semibold text-[var(--on-surface)] mt-0.5">
                        {selectedSession.cache_hits}
                    </p>
                </div>
            </div>

            {#if selectedSession.mcp_tool_calls.length > 0}
                <div class="space-y-2">
                    <h3 class="text-xs font-semibold uppercase tracking-wider text-[var(--on-surface-variant)]">
                        MCP Tool Calls
                    </h3>
                    <div class="overflow-x-auto rounded-lg border border-[var(--ghost-border)]">
                        <table class="w-full text-left text-xs">
                            <thead>
                                <tr class="bg-[var(--surface-container-low)] text-[var(--on-surface-variant)]">
                                    <th class="px-3 py-2">Server</th>
                                    <th class="px-3 py-2">Tool</th>
                                    <th class="px-3 py-2">Calls</th>
                                    <th class="px-3 py-2">Avg Latency</th>
                                    <th class="px-3 py-2">Errors</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-[var(--ghost-border)] font-mono">
                                {#each selectedSession.mcp_tool_calls as call}
                                    <tr>
                                        <td class="px-3 py-2 text-[var(--on-surface)]">{call.server}</td>
                                        <td class="px-3 py-2 text-[var(--on-surface)]">{call.tool}</td>
                                        <td class="px-3 py-2">{call.count}</td>
                                        <td class="px-3 py-2">{call.avg_latency_ms} ms</td>
                                        <td class="px-3 py-2">{call.errors}</td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                </div>
            {/if}

            <div class="space-y-2">
                <h3 class="text-xs font-semibold uppercase tracking-wider text-[var(--on-surface-variant)]">
                    Redacted Trajectory Timeline ({selectedSession.trajectory.length} events)
                </h3>
                {#if selectedSession.trajectory.length === 0}
                    <p class="text-xs text-[var(--on-surface-variant)]">
                        No trajectory events recorded for this session.
                    </p>
                {:else}
                    <div class="space-y-2">
                        {#each selectedSession.trajectory as ev}
                            <div class="p-3 rounded-lg bg-[var(--surface-container-low)] space-y-1.5">
                                <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
                                    <div class="flex items-center gap-2">
                                        <span class="font-mono text-[var(--on-surface-variant)]">#{ev.step}</span>
                                        <Badge variant={eventKindVariant(ev.kind)} label={ev.kind} />
                                        <span class="font-medium text-[var(--on-surface)]">{ev.role}</span>
                                        {#if ev.mcp_server && ev.tool_name}
                                            <span class="font-mono text-[var(--primary)]">
                                                {ev.mcp_server}.{ev.tool_name}
                                            </span>
                                        {/if}
                                        {#if ev.cache_hit}
                                            <Badge variant="success" label="cache hit" />
                                        {/if}
                                    </div>
                                    <div class="font-mono text-[var(--on-surface-variant)]">
                                        {#if ev.tokens}
                                            {ev.tokens} tok
                                        {/if}
                                        {#if ev.latency_ms}
                                            · {ev.latency_ms} ms
                                        {/if}
                                    </div>
                                </div>
                                <pre class="text-xs font-mono text-[var(--on-surface)] whitespace-pre-wrap break-words">{ev.content}</pre>
                            </div>
                        {/each}
                    </div>
                {/if}
            </div>
        </div>
    {/if}
    </div>
</Modal>
