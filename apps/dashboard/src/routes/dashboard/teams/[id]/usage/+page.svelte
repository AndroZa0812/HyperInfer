<script lang="ts">
    import { page } from '$app/stores';
import { api } from '$lib/api';
    import type { UsageData } from '$lib/types';
    import UsageChart from '$lib/components/UsageChart.svelte';

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
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold">Usage Analytics</h1>
        <select bind:value={period} class="px-4 py-2 border rounded-lg">
            <option value="7d">Last 7 days</option>
            <option value="30d">Last 30 days</option>
            <option value="90d">Last 90 days</option>
        </select>
    </div>

    {#if loading}
        <p>Loading...</p>
    {:else}
        <div class="grid grid-cols-4 gap-4">
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Total Tokens</p>
                <p class="text-2xl font-bold">{totalTokens.toLocaleString()}</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Total Spend</p>
                <p class="text-2xl font-bold">${totalCost.toFixed(2)}</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Avg Latency</p>
                <p class="text-2xl font-bold">{avgLatency.toFixed(0)}ms</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Uptime</p>
                <p class="text-2xl font-bold">99.9%</p>
            </div>
        </div>

        <div class="grid grid-cols-2 gap-4">
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <h2 class="text-lg font-semibold mb-4">Token Usage Over Time</h2>
                <UsageChart {data} type="line" />
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <h2 class="text-lg font-semibold mb-4">Cost by Model</h2>
                <UsageChart {data} type="bar" />
            </div>
        </div>
    {/if}
</div>