<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { Team, UsageData } from '$lib/types';

    let team = $state<Team | null>(null);
    let usageData = $state<UsageData[]>([]);
    let loading = $state(true);

    let teamId = $derived($page.params.id);

    let usedCents = $derived(usageData.reduce((sum, d) => sum + d.cost, 0));
    let usedPercent = $derived(team && team.budget_cents > 0 ? Math.min(usedCents / team.budget_cents, 1) : 0);

    $effect(() => {
        if (!teamId) return;
        let cancelled = false;
        loading = true;
        Promise.all([api.getTeam(teamId), api.getUsage(teamId, '30d').catch(() => [])])
            .then(([t, u]) => {
                if (cancelled) return;
                team = t;
                usageData = u;
                loading = false;
            })
            .catch((e) => {
                if (cancelled) return;
                console.error('Failed to load team budget', e);
                team = null;
                usageData = [];
                loading = false;
            });
        return () => { cancelled = true; };
    });
</script>

{#if loading}
    <p>Loading...</p>
{:else if team}
    <div class="space-y-6">
        <h1 class="text-2xl font-bold">Budget - {team.name}</h1>

        <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
            <h2 class="text-lg font-semibold mb-4">Budget Progress</h2>
            <div class="h-4 bg-gray-200 rounded-full overflow-hidden">
                <div class="h-full bg-[var(--accent)]" style="width: {usedPercent * 100}%"></div>
            </div>
            <p class="text-sm text-gray-500 mt-2">${(usedCents / 100).toFixed(2)} of ${(team.budget_cents / 100).toFixed(2)} used ({(usedPercent * 100).toFixed(1)}%)</p>
        </div>
    </div>
{:else}
    <p>Team not found</p>
{/if}