<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { ApiKey, Team, UsageLog, UsageLogsFilter } from '#lib/types';
    import { onDestroy } from 'svelte';
    import Card from '@hyperinfer/ui/Card.svelte';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';
    import StatCard from '#lib/components/StatCard.svelte';
    import Modal from '@hyperinfer/ui/Modal.svelte';

    let logs = $state<UsageLog[]>([]);
    let total = $state(0);
    let limit = $state(25);
    let offset = $state(0);
    let loading = $state(true);
    let error = $state('');

    let teams = $state<Team[]>([]);
    let keys = $state<ApiKey[]>([]);

    // Filter state
    let teamFilter = $state('');
    let keyFilter = $state('');
    let modelFilter = $state('');
    let statusFilter = $state('');
    let cacheFilter = $state<'all' | 'hit' | 'miss'>('all');
    let fromDate = $state('');
    let toDate = $state('');
    let liveTail = $state(false);
    let pollTimer: ReturnType<typeof setInterval> | null = null;

    let selectedLog = $state<UsageLog | null>(null);
    let showDetail = $state(false);

    let isAdmin = $derived($auth.user?.role === 'admin');

    let avgLatencyMs = $derived(
        logs.length > 0
            ? Math.round(logs.reduce((acc, l) => acc + l.response_time_ms, 0) / logs.length)
            : 0
    );
    let cacheHitPct = $derived(
        logs.length > 0
            ? Math.round((logs.filter((l) => l.cache_hit).length / logs.length) * 100)
            : 0
    );
    let pageCostUsd = $derived(
        logs.reduce((acc, l) => acc + (l.cost_cents || 0), 0) / 100
    );

    let initialized = false;

    $effect(() => {
        if ($auth.loading || !$auth.user || initialized) return;
        initialized = true;
        const teamScope = isAdmin ? undefined : $auth.user.team_id;
        Promise.allSettled([
            isAdmin ? api.getTeams() : Promise.resolve([]),
            api.getKeys(teamScope),
        ]).then(([tRes, kRes]) => {
            if (tRes.status === 'fulfilled') teams = tRes.value;
            if (kRes.status === 'fulfilled') keys = kRes.value;
        });
        loadLogs();
    });

    $effect(() => {
        if (pollTimer) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
        if (liveTail) {
            pollTimer = setInterval(() => {
                loadLogs(true);
            }, 5000);
        }
        return () => {
            if (pollTimer) {
                clearInterval(pollTimer);
                pollTimer = null;
            }
        };
    });

    onDestroy(() => {
        if (pollTimer) clearInterval(pollTimer);
    });

    function buildFilter(): UsageLogsFilter {
        const f: UsageLogsFilter = { limit, offset };
        if (isAdmin && teamFilter) f.team_id = teamFilter;
        if (keyFilter) f.api_key_id = keyFilter;
        if (modelFilter.trim()) f.model = modelFilter.trim();
        if (statusFilter.trim()) {
            const code = Number(statusFilter);
            if (Number.isFinite(code) && code > 0) f.status_code = Math.round(code);
        }
        if (cacheFilter === 'hit') f.cache_hit = true;
        if (cacheFilter === 'miss') f.cache_hit = false;
        if (fromDate) f.from = new Date(`${fromDate}T00:00:00Z`).toISOString();
        if (toDate) f.to = new Date(`${toDate}T23:59:59Z`).toISOString();
        return f;
    }

    async function loadLogs(silent = false) {
        if (!silent) loading = true;
        error = '';
        try {
            const page = await api.getUsageLogs(buildFilter());
            logs = page.logs;
            total = page.total;
        } catch (e) {
            console.error('Failed to load request logs', e);
            error = 'Failed to load request logs.';
        } finally {
            loading = false;
        }
    }

    function applyFilters() {
        offset = 0;
        loadLogs();
    }

    function resetFilters() {
        teamFilter = '';
        keyFilter = '';
        modelFilter = '';
        statusFilter = '';
        cacheFilter = 'all';
        fromDate = '';
        toDate = '';
        offset = 0;
        loadLogs();
    }

    function prevPage() {
        offset = Math.max(0, offset - limit);
        loadLogs();
    }

    function nextPage() {
        if (offset + limit < total) {
            offset += limit;
            loadLogs();
        }
    }

    function openDetail(log: UsageLog) {
        selectedLog = log;
        showDetail = true;
    }

    function statusVariant(code: number): 'success' | 'warning' | 'error' {
        if (code >= 200 && code < 300) return 'success';
        if (code >= 400 && code < 500) return 'warning';
        return 'error';
    }
</script>

<div class="space-y-8">
    <div class="flex flex-wrap items-center justify-between gap-4">
        <div>
            <h1 class="text-2xl font-bold text-[var(--on-surface)]">Request Logs</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                Inspect request latency, token breakdown, weighted cost, cache hits, and failover telemetry. Raw prompts/responses are omitted to protect redacted PII.
            </p>
        </div>
        <div class="flex items-center gap-3">
            <label class="flex items-center gap-2 text-xs font-medium text-[var(--on-surface-variant)] cursor-pointer select-none">
                <input type="checkbox" bind:checked={liveTail} />
                Live Tail (5s)
            </label>
            <Button variant="secondary" size="sm" onclick={() => loadLogs()}>
                <span class="material-symbols-outlined" style="font-size: 18px">refresh</span>
                Refresh
            </Button>
        </div>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <StatCard icon="receipt_long" label="Matching Requests" value={total.toLocaleString()} />
        <StatCard icon="timer" label="Avg Latency (Page)" value={`${avgLatencyMs}ms`} />
        <StatCard icon="bolt" label="Cache Hit Rate" value={`${cacheHitPct}%`} />
        <StatCard icon="payments" label="Page Est. Cost" value={`$${pageCostUsd.toFixed(4)}`} />
    </div>

    <!-- Filter Bar -->
    <Card>
        <form class="space-y-4" onsubmit={(e) => { e.preventDefault(); applyFilters(); }}>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 2xl:grid-cols-7 gap-3 items-end">
                {#if isAdmin}
                    <div class="space-y-1.5">
                        <label for="logs-team" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Team</label>
                        <select
                            id="logs-team"
                            bind:value={teamFilter}
                            class="h-10 w-full px-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                        >
                            <option value="">All Teams</option>
                            {#each teams as t}
                                <option value={t.id}>{t.name}</option>
                            {/each}
                        </select>
                    </div>
                {/if}

                <div class="space-y-1.5">
                    <label for="logs-key" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">API Key</label>
                    <select
                        id="logs-key"
                        bind:value={keyFilter}
                        class="h-10 w-full px-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                    >
                        <option value="">All Keys</option>
                        {#each keys as k}
                            <option value={k.id}>{k.name || k.id.slice(0, 8)}</option>
                        {/each}
                    </select>
                </div>

                <Input id="logs-model" label="Model" bind:value={modelFilter} placeholder="e.g. claude" />

                <div class="space-y-1.5">
                    <label for="logs-status" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</label>
                    <select
                        id="logs-status"
                        bind:value={statusFilter}
                        class="h-10 w-full px-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                    >
                        <option value="">All Status</option>
                        <option value="200">200 OK</option>
                        <option value="400">400 Bad Request</option>
                        <option value="401">401 Unauthorized</option>
                        <option value="429">429 Rate Limited</option>
                        <option value="500">500 Server Error</option>
                        <option value="502">502 Bad Gateway</option>
                    </select>
                </div>

                <div class="space-y-1.5">
                    <label for="logs-cache" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Cache</label>
                    <select
                        id="logs-cache"
                        bind:value={cacheFilter}
                        class="h-10 w-full px-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border border-[var(--outline-variant)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)] outline-none text-sm transition-colors duration-200"
                    >
                        <option value="all">All</option>
                        <option value="hit">Cache HIT</option>
                        <option value="miss">Cache MISS</option>
                    </select>
                </div>

                <Input id="logs-from" label="From" type="date" bind:value={fromDate} />
                <Input id="logs-to" label="To" type="date" bind:value={toDate} />
            </div>

            <div class="flex justify-end gap-2">
                <Button variant="secondary" size="sm" onclick={resetFilters}>Reset</Button>
                <Button type="submit" size="sm">Apply Filters</Button>
            </div>
        </form>
    </Card>

    <!-- Logs Table -->
    <Card padding="none">
        {#if loading}
            <div class="flex items-center justify-center h-40">
                <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 26px">progress_activity</span>
            </div>
        {:else if error}
            <div class="p-8 text-center text-sm text-[var(--error)]">{error}</div>
        {:else if logs.length === 0}
            <div class="p-10 text-center text-sm text-[var(--on-surface-variant)]">
                No request logs match the current filters.
            </div>
        {:else}
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Timestamp</th>
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Model</th>
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Latency</th>
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Tokens (In / Cached / Out)</th>
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Cost</th>
                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Cache &amp; Routing</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each logs as log, i}
                            <tr
                                class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/60 transition-colors cursor-pointer"
                                onclick={() => openDetail(log)}
                            >
                                <td class="px-4 py-3 text-xs font-mono text-[var(--on-surface-variant)]">
                                    {new Date(log.recorded_at).toLocaleString()}
                                </td>
                                <td class="px-4 py-3 text-sm font-mono font-medium text-[var(--on-surface)]">
                                    {log.model}
                                </td>
                                <td class="px-4 py-3">
                                    <Badge variant={statusVariant(log.status_code)} label={String(log.status_code)} />
                                </td>
                                <td class="px-4 py-3 text-xs font-mono text-[var(--on-surface)]">
                                    {log.response_time_ms}ms
                                </td>
                                <td class="px-4 py-3 text-xs font-mono text-[var(--on-surface-variant)]">
                                    {log.input_tokens.toLocaleString()} / {log.cached_input_tokens.toLocaleString()} / {log.output_tokens.toLocaleString()}
                                    {#if log.reasoning_tokens > 0}
                                        <span class="text-[var(--primary)]"> (+{log.reasoning_tokens} rsn)</span>
                                    {/if}
                                </td>
                                <td class="px-4 py-3 text-xs font-mono text-[var(--on-surface)]">
                                    {'$'}{(log.cost_cents / 100).toFixed(4)}
                                </td>
                                <td class="px-4 py-3">
                                    <div class="flex flex-wrap items-center gap-1.5">
                                        <Badge variant={log.cache_hit ? 'success' : 'neutral'} label={log.cache_hit ? 'HIT' : 'MISS'} />
                                        {#if log.failover_used}
                                            <Badge variant="warning" label="Failover" />
                                        {/if}
                                        {#if log.deployment_name}
                                            <span class="text-xs font-mono text-[var(--on-surface-variant)]">{log.deployment_name}</span>
                                        {/if}
                                    </div>
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
            <div class="px-5 py-3 flex items-center justify-between border-t border-[var(--ghost-border)]">
                <span class="text-xs text-[var(--on-surface-variant)]">
                    Showing {offset + 1}–{Math.min(offset + logs.length, total)} of {total}
                </span>
                <div class="flex gap-2">
                    <Button variant="ghost" size="sm" disabled={offset === 0} onclick={prevPage}>Previous</Button>
                    <Button variant="ghost" size="sm" disabled={offset + limit >= total} onclick={nextPage}>Next</Button>
                </div>
            </div>
        {/if}
    </Card>
</div>

<Modal bind:open={showDetail}>
    {#if selectedLog}
        <div class="p-6 space-y-5">
            <div class="flex items-center justify-between">
                <div>
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Request Metadata</h2>
                    <p class="text-xs font-mono text-[var(--on-surface-variant)]">{selectedLog.id}</p>
                </div>
                <Badge variant={statusVariant(selectedLog.status_code)} label={`HTTP ${selectedLog.status_code}`} />
            </div>

            <div class="grid grid-cols-2 gap-3 text-xs">
                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Model</p>
                    <p class="font-mono font-semibold text-[var(--on-surface)] mt-0.5">{selectedLog.model}</p>
                </div>
                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Latency</p>
                    <p class="font-mono font-semibold text-[var(--on-surface)] mt-0.5">{selectedLog.response_time_ms}ms</p>
                </div>
                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Tokens (Input / Cached / Output / Reasoning)</p>
                    <p class="font-mono text-[var(--on-surface)] mt-0.5">
                        {selectedLog.input_tokens} / {selectedLog.cached_input_tokens} / {selectedLog.output_tokens} / {selectedLog.reasoning_tokens}
                    </p>
                </div>
                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Estimated Cost</p>
                    <p class="font-mono font-semibold text-[var(--on-surface)] mt-0.5">{'$'}{(selectedLog.cost_cents / 100).toFixed(5)}</p>
                </div>
                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Deployment / Failover</p>
                    <p class="font-mono text-[var(--on-surface)] mt-0.5">
                        {selectedLog.deployment_name || 'default'} {selectedLog.failover_used ? '(Failover)' : ''}
                    </p>
                </div>
                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                    <p class="text-[var(--on-surface-variant)]">Cache / Stop Reason</p>
                    <p class="font-mono text-[var(--on-surface)] mt-0.5">
                        {selectedLog.cache_hit ? 'HIT' : 'MISS'} · {selectedLog.stop_reason || 'n/a'}
                    </p>
                </div>
            </div>

            <div class="flex justify-end">
                <Button size="sm" onclick={() => showDetail = false}>Close</Button>
            </div>
        </div>
    {/if}
</Modal>
