<script lang="ts">
    import { page } from '$app/state';
    import { api } from '#lib/api';
    import type { UsageData } from '#lib/types';
    import StatCard from '#lib/components/StatCard.svelte';
    import Card from '@hyperinfer/ui/Card.svelte';

    let summary = $state<UsageData | null>(null);
    let loading = $state(true);

    let teamId = $derived(page.params.id);

    $effect(() => {
        if (!teamId) return;
        let cancelled = false;
        loading = true;
        api.getUsage(teamId)
            .then((u) => {
                if (cancelled) return;
                summary = u;
                loading = false;
            })
            .catch((e) => {
                if (cancelled) return;
                console.error('Failed to load usage', e);
                summary = null;
                loading = false;
            });
        return () => { cancelled = true; };
    });
</script>

<div class="space-y-8">
    <div>
        <h1 class="text-3xl font-bold text-[var(--on-surface)]">Usage Analytics</h1>
        <p class="text-sm text-[var(--on-surface-variant)] mt-1">Monitoring cluster performance and resource allocation</p>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-64">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
        </div>
    {:else}
        <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-4">
            <StatCard icon="speed" label="Total Requests" value={summary ? summary.total_requests.toLocaleString() : '0'} />
            <StatCard icon="input" label="Input Tokens" value={summary ? summary.total_input_tokens.toLocaleString() : '0'} />
            <StatCard icon="output" label="Output Tokens" value={summary ? summary.total_output_tokens.toLocaleString() : '0'} />
            <StatCard icon="timer" label="Avg Latency" value={summary ? `${Math.round(summary.avg_response_time_ms)}ms` : '0ms'} />
        </div>

        <Card>
            <div class="space-y-4">
                <div>
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Usage Over Time</h2>
                    <p class="text-xs text-[var(--on-surface-variant)]">Time-series usage data</p>
                </div>
                {#if summary && summary.total_requests > 0}
                    <div class="h-64 flex items-center justify-center text-[var(--on-surface-variant)] text-sm">
                        Time-series chart data will appear here as usage is recorded.
                    </div>
                {:else}
                    <div class="h-64 flex items-center justify-center text-[var(--on-surface-variant)] text-sm">
                        No time-series usage data recorded yet
                    </div>
                {/if}
            </div>
        </Card>
    {/if}
</div>