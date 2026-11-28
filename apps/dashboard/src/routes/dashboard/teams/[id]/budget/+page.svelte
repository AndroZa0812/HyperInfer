<script lang="ts">
    import { page } from '$app/state';
    import { api } from '#lib/api';
    import type { Team, UsageData } from '#lib/types';
    import Card from '@hyperinfer/ui/Card.svelte';
    import Breadcrumbs from '#lib/components/Breadcrumbs.svelte';
    import StatCard from '#lib/components/StatCard.svelte';

    let team = $state<Team | null>(null);
    let usage = $state<UsageData | null>(null);
    let loading = $state(true);

    let teamId = $derived(page.params.id);

    $effect(() => {
        if (!teamId) return;
        let cancelled = false;
        loading = true;
        Promise.all([api.getTeam(teamId), api.getUsage(teamId).catch(() => null)])
            .then(([t, u]) => {
                if (cancelled) return;
                team = t;
                usage = u;
                loading = false;
            })
            .catch((e) => {
                if (cancelled) return;
                console.error('Failed to load team budget', e);
                team = null;
                usage = null;
                loading = false;
            });
        return () => { cancelled = true; };
    });
</script>

{#if loading}
    <div class="flex items-center justify-center h-64">
        <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
    </div>
{:else if team}
    <div class="space-y-8">
        <Breadcrumbs items={[
            { label: 'Teams', href: '/dashboard/teams' },
            { label: team.name, href: `/dashboard/teams/${teamId}` },
            { label: 'Budget' }
        ]} />

        <h1 class="text-3xl font-bold text-[var(--on-surface)]">Budget - {team.name}</h1>

        <Card>
            <div class="space-y-6">
                <h2 class="text-lg font-semibold text-[var(--on-surface)]">Monthly Budget</h2>
                <div>
                    <span class="text-3xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">
                        {'$'}{(team.budget_cents / 100).toFixed(2)}
                    </span>
                    <span class="text-sm text-[var(--on-surface-variant)] ml-2">per month</span>
                </div>
            </div>
        </Card>

        <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-4">
            <StatCard icon="speed" label="Total Requests" value={usage ? usage.total_requests.toLocaleString() : '0'} />
            <StatCard icon="security" label="Total Tokens" value={usage ? (usage.total_input_tokens + usage.total_output_tokens).toLocaleString() : '0'} />
            <StatCard icon="timer" label="Average Latency" value={usage ? `${Math.round(usage.avg_response_time_ms)}ms` : '0ms'} />
        </div>
    </div>
{:else}
    <p class="text-[var(--on-surface-variant)]">Team not found</p>
{/if}