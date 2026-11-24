<script lang="ts">
    import { page } from '$app/state';
    import { api } from '$lib/api';
    import type { Team, ApiKey, UsageData } from '$lib/types';
    import StatCard from '$lib/components/StatCard.svelte';
    import Card from '$lib/components/Card.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import Breadcrumbs from '$lib/components/Breadcrumbs.svelte';
    import Button from '$lib/components/Button.svelte';

    let team = $state<Team | null>(null);
    let keys = $state<ApiKey[]>([]);
    let usage = $state<UsageData | null>(null);
    let loading = $state(true);
    let error = $state<string | null>(null);

    let teamId = $derived(page.params.id);

    $effect(() => {
        if (!teamId) return;
        let cancelled = false;
        loading = true;
        error = null;
        Promise.all([
            api.getTeam(teamId),
            api.getKeys(teamId).catch(() => []),
            api.getUsage(teamId).catch(() => null),
        ])
            .then(([t, k, u]) => {
                if (cancelled) return;
                team = t;
                keys = k;
                usage = u;
                loading = false;
            })
            .catch((e) => {
                if (cancelled) return;
                console.error('Failed to load team', e);
                error = 'Failed to load team';
                loading = false;
            });
        return () => { cancelled = true; };
    });

    let activeKeys = $derived(keys.filter(k => k.is_active).length);
    let totalTokens = $derived(usage ? usage.total_input_tokens + usage.total_output_tokens : 0);
</script>

{#if loading}
    <div class="flex items-center justify-center h-64">
        <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
    </div>
{:else if error}
    <div class="flex items-center gap-2 text-[var(--error)]">
        <span class="material-symbols-outlined">error</span>
        {error}
    </div>
{:else if team}
    <div class="space-y-8">
        <Breadcrumbs items={[
            { label: 'Teams', href: '/dashboard/teams' },
            { label: team.name }
        ]} />

        <div class="flex items-center justify-between">
            <div class="flex items-center gap-4">
                <div>
                    <div class="flex items-center gap-3">
                        <h1 class="text-3xl font-bold text-[var(--on-surface)]">{team.name}</h1>
                        <Badge variant="info" label="Admin" />
                    </div>
                </div>
            </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCard icon="vpn_key" label="Total Keys" value={keys.length} trend="API keys configured" trendDirection="neutral" />
            <StatCard icon="sensors" label="Active Keys" value={activeKeys} trend={`${activeKeys > 0 ? Math.round((activeKeys / keys.length) * 100) : 0}% connectivity`} trendDirection="up" />
            <StatCard icon="data_usage" label="Usage This Month" value={totalTokens.toLocaleString()} trend="Tokens processed" trendDirection="neutral" />
            <StatCard icon="account_balance_wallet" label="Budget Left" value={`$${(team.budget_cents / 100).toFixed(2)}`} trend="Monthly budget" trendDirection="neutral" />
        </div>

        <Card>
            <div class="space-y-6">
                <div>
                    <h2 class="text-lg font-semibold text-[var(--on-surface)]">Monthly Budget</h2>
                    <p class="text-sm text-[var(--on-surface-variant)]">Team: {team.name}</p>
                </div>

                <div>
                    <div class="flex items-end justify-between mb-3">
                        <span class="text-3xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">
                            {'$'}{(team.budget_cents / 100).toFixed(2)}
                        </span>
                        <span class="text-sm text-[var(--on-surface-variant)]">
                            Monthly budget
                        </span>
                    </div>
                    <div class="flex gap-8 mt-4">
                        <div>
                            <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">Total Requests</p>
                            <p class="text-sm font-medium text-[var(--on-surface)]">{usage ? usage.total_requests.toLocaleString() : '0'}</p>
                        </div>
                        <div>
                            <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">Avg Latency</p>
                            <p class="text-sm font-medium text-[var(--on-surface)]">{usage ? `${usage.avg_response_time_ms}ms` : '—'}</p>
                        </div>
                    </div>
                </div>
            </div>
        </Card>

        <div class="flex gap-3">
            <Button href="/dashboard/teams/{team.id}/keys" variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">vpn_key</span>
                Manage Keys
            </Button>
            <Button href="/dashboard/teams/{team.id}/usage" variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">bar_chart</span>
                Usage
            </Button>
            <Button href="/dashboard/teams/{team.id}/budget" variant="secondary" size="sm">
                <span class="material-symbols-outlined" style="font-size: 18px">account_balance_wallet</span>
                Budget
            </Button>
        </div>
    </div>
{:else}
    <p class="text-[var(--on-surface-variant)]">Team not found</p>
{/if}