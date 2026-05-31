<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { Team, UsageData } from '$lib/types';
    import Card from '$lib/components/Card.svelte';
    import Breadcrumbs from '$lib/components/Breadcrumbs.svelte';

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
                <h2 class="text-lg font-semibold text-[var(--on-surface)]">Budget Progress</h2>

                <div>
                    <div class="flex items-end justify-between mb-3">
                        <span class="text-3xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">
                            {'$'}{(usedCents / 100).toFixed(2)}
                        </span>
                        <span class="text-sm text-[var(--on-surface-variant)]">
                            of {'$'}{(team.budget_cents / 100).toFixed(2)} used ({(usedPercent * 100).toFixed(1)}%)
                        </span>
                    </div>
                    <div class="h-4 bg-[var(--surface-container)] rounded-full overflow-hidden">
                        <div class="h-full rounded-full transition-all duration-500" style="width: {usedPercent * 100}%; background: linear-gradient(90deg, var(--primary), var(--primary-container))"></div>
                    </div>
                </div>
            </div>
        </Card>
    </div>
{:else}
    <p class="text-[var(--on-surface-variant)]">Team not found</p>
{/if}
