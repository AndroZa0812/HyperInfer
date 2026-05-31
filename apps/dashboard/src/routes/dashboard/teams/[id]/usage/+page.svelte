<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { UsageData } from '$lib/types';
    import UsageChart from '$lib/components/UsageChart.svelte';
    import StatCard from '$lib/components/StatCard.svelte';
    import Card from '$lib/components/Card.svelte';
    import Button from '$lib/components/Button.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import SegmentedControl from '$lib/components/SegmentedControl.svelte';

    let data = $state<UsageData[]>([]);
    let loading = $state(true);
    let period = $state('30d');

    let teamId = $derived($page.params.id);

    let totalTokens = $derived(data.reduce((sum, d) => sum + d.tokens, 0));
    let totalCost = $derived(data.reduce((sum, d) => sum + d.cost, 0));
    let avgLatency = $derived(data.length ? data.reduce((sum, d) => sum + d.latency_ms, 0) / data.length : 0);

    let requestId = 0;

    async function loadData() {
        if (!teamId) return;
        const currentRequestId = ++requestId;
        loading = true;
        try {
            const result = await api.getUsage(teamId, period);
            if (currentRequestId === requestId) {
                data = result;
            }
        } catch (e) {
            console.error('Failed to load usage', e);
        } finally {
            if (currentRequestId === requestId) {
                loading = false;
            }
        }
    }

    $effect(() => {
        if (teamId && period) loadData();
    });

    const periodItems = [
        { label: '7d', value: '7d' },
        { label: '30d', value: '30d' },
        { label: '90d', value: '90d' },
    ];

    const costByModel = [
        { name: 'HyperInfer 400B', cost: 840 },
        { name: 'HyperInfer 70B', cost: 320 },
        { name: 'HyperInfer 8B (Vision)', cost: 80 },
    ];

    const clusterDist = [
        { name: 'Prod-Cluster-Alpha', pct: 45 },
        { name: 'Staging-Beta', pct: 25 },
        { name: 'Dev-Local-01', pct: 15 },
        { name: 'Others', pct: 15 },
    ];

    const anomalyLogs = [
        { timestamp: 'Oct 24, 14:22:01', key: 'hk_live_...94c2', event: 'Spike: Batch Inference', duration: '1.2s', status: 'Resolved' },
        { timestamp: 'Oct 24, 13:05:44', key: 'hk_live_...f110', event: 'High Latency (400B)', duration: '4.5s', status: 'Monitoring' },
    ];
</script>

<div class="space-y-8">
    <div class="flex items-center justify-between">
        <div>
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">Usage Analytics</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Monitoring cluster performance and resource allocation</p>
        </div>
        <div class="flex items-center gap-4">
            <SegmentedControl items={periodItems} bind:active={period} />
            <Button variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">download</span>
                Export CSV
            </Button>
        </div>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-64">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
        </div>
    {:else}
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCard icon="toll" label="Total Tokens" value="{(totalTokens / 1e6).toFixed(1)}M" trend="+12%" trendDirection="up" />
            <StatCard icon="payments" label="Total Spend" value="${'$'}{totalCost.toFixed(0)}" trend="Budgeted" trendDirection="neutral" />
            <StatCard icon="timer" label="Avg Latency" value="{avgLatency.toFixed(0)}ms" trend="-4ms" trendDirection="up" />
            <StatCard icon="cloud_done" label="Uptime" value="99.99%" trend="Stable" trendDirection="neutral" />
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <Card>
                <div class="flex items-center justify-between mb-4">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">Token Usage</h2>
                        <p class="text-xs text-[var(--on-surface-variant)]">Last 30 days throughput</p>
                    </div>
                    <Badge variant="info" label="Peak: 1.2M" />
                </div>
                <UsageChart {data} type="line" />
            </Card>

            <Card>
                <div class="mb-4">
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Cost by Model</h2>
                </div>
                <div class="space-y-4">
                    {#each costByModel as model}
                        <div class="flex items-center justify-between">
                            <div class="flex items-center gap-3">
                                <div class="w-3 h-3 rounded-full" style="background: linear-gradient(135deg, var(--primary), var(--primary-container))"></div>
                                <span class="text-sm text-[var(--on-surface)]">{model.name}</span>
                            </div>
                            <span class="text-sm font-semibold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">${'$'}{model.cost.toFixed(2)}</span>
                        </div>
                        <div class="h-2 bg-[var(--surface-container)] rounded-full overflow-hidden">
                            <div class="h-full rounded-full" style="width: {(model.cost / 840 * 100).toFixed(0)}%; background: linear-gradient(90deg, var(--primary), var(--primary-container))"></div>
                        </div>
                    {/each}
                </div>
            </Card>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <Card>
                <div class="mb-4">
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Distribution by Cluster</h2>
                </div>
                <div class="space-y-3">
                    {#each clusterDist as cluster}
                        <div class="flex items-center justify-between">
                            <span class="text-sm text-[var(--on-surface)]">{cluster.name}</span>
                            <span class="text-sm font-medium text-[var(--on-surface-variant)]">{cluster.pct}%</span>
                        </div>
                        <div class="h-2 bg-[var(--surface-container)] rounded-full overflow-hidden">
                            <div class="h-full rounded-full bg-[var(--secondary)]" style="width: {cluster.pct}%"></div>
                        </div>
                    {/each}
                </div>
            </Card>

            <Card>
                <div class="mb-4">
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Latency Distribution (ms)</h2>
                </div>
                <UsageChart {data} type="bar" />
            </Card>
        </div>

        <Card>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">Anomalous Activity Logs</h2>
                    </div>
                    <Button variant="ghost" size="sm">View All Logs</Button>
                </div>
                <div class="overflow-x-auto">
                    <table class="w-full">
                        <thead>
                            <tr class="bg-[var(--surface-container-low)]">
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Timestamp</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">API Key ID</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Event Type</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Duration</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each anomalyLogs as log, i}
                                <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''}">
                                    <td class="px-5 py-3.5 text-sm font-mono text-[var(--on-surface-variant)]">{log.timestamp}</td>
                                    <td class="px-5 py-3.5 text-sm font-mono text-[var(--on-surface-variant)]">{log.key}</td>
                                    <td class="px-5 py-3.5 text-sm text-[var(--on-surface)]">{log.event}</td>
                                    <td class="px-5 py-3.5 text-sm text-[var(--on-surface)]">{log.duration}</td>
                                    <td class="px-5 py-3.5">
                                        <Badge variant={log.status === 'Resolved' ? 'success' : 'warning'} label={log.status} />
                                    </td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            </div>
        </Card>
    {/if}
</div>
