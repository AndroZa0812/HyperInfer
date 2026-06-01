<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { Team } from '$lib/types';
    import StatCard from '$lib/components/StatCard.svelte';
    import Card from '$lib/components/Card.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import Breadcrumbs from '$lib/components/Breadcrumbs.svelte';
    import Button from '$lib/components/Button.svelte';

    let team = $state<Team | null>(null);
    let loading = $state(true);
    let error = $state<string | null>(null);

    let teamId = $derived($page.params.id);

    $effect(() => {
        if (teamId) {
            loading = true;
            error = null;
            api.getTeam(teamId)
                .then((t) => {
                    team = t;
                    loading = false;
                })
                .catch((e) => {
                    console.error('Failed to load team', e);
                    error = 'Failed to load team';
                    loading = false;
                });
        }
    });

    const activityLog = [
        { timestamp: 'Oct 24, 14:22:10', key: 'Production-Main', action: 'Inference', tokens: '1,240' },
        { timestamp: 'Oct 24, 14:18:45', key: 'Staging-API-V2', action: 'Embedding', tokens: '4,812' },
        { timestamp: 'Oct 24, 14:05:01', key: 'Dev-Test-Lambda', action: 'Inference', tokens: '240' },
        { timestamp: 'Oct 24, 13:58:22', key: 'Production-Main', action: 'Fine-Tune', tokens: '0' },
        { timestamp: 'Oct 24, 13:42:15', key: 'Legacy-Web-App', action: 'Inference', tokens: '892' },
    ];
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
            <div class="flex gap-3">
                <Button variant="secondary" size="sm">
                    <span class="material-symbols-outlined" style="font-size: 18px">edit</span>
                    Edit Team
                </Button>
                <Button size="sm">
                    <span class="material-symbols-outlined" style="font-size: 18px">vpn_key</span>
                    Generate New Key
                </Button>
            </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCard icon="vpn_key" label="Total Keys" value="42" trend="12% from last month" trendDirection="up" />
            <StatCard icon="sensors" label="Active Keys" value="38" trend="90.4% connectivity" trendDirection="up" />
            <StatCard icon="data_usage" label="Usage This Month" value="14.2M" trend="Tokens processed" trendDirection="neutral" />
            <StatCard icon="account_balance_wallet" label="Budget Left" value="${'$'}{(team.budget_cents / 100).toFixed(0)}" trend="Renewal in 12 days" trendDirection="neutral" />
        </div>

        <Card>
            <div class="space-y-6">
                <div class="flex items-center justify-between">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">Monthly Budget Allocation</h2>
                        <p class="text-sm text-[var(--on-surface-variant)]">Team: {team.name}</p>
                    </div>
                    <Button variant="ghost" size="sm">
                        <span class="material-symbols-outlined" style="font-size: 18px">tune</span>
                        Adjust Limits
                    </Button>
                </div>

                <div>
                    <div class="flex items-end justify-between mb-3">
                        <span class="text-3xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">
                            {'$'}{(team.budget_cents * 0.752 / 100).toFixed(0)}
                        </span>
                        <span class="text-sm text-[var(--on-surface-variant)]">
                            / {'$'}{(team.budget_cents / 100).toFixed(0)} remaining
                        </span>
                    </div>
                    <div class="h-3 bg-[var(--surface-container)] rounded-full overflow-hidden">
                        <div class="h-full rounded-full transition-all duration-500" style="width: 75.2%; background: linear-gradient(90deg, var(--primary), var(--primary-container))"></div>
                    </div>
                    <div class="flex gap-8 mt-4">
                        <div>
                            <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">Burn Rate</p>
                            <p class="text-sm font-medium text-[var(--on-surface)]">{'$'}142.20 /day</p>
                        </div>
                        <div>
                            <p class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">Est. Exhaustion</p>
                            <p class="text-sm font-medium text-[var(--on-surface)]">26 Days</p>
                        </div>
                    </div>
                </div>
            </div>
        </Card>

        <Card>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <div>
                        <h2 class="text-lg font-semibold text-[var(--on-surface)]">Recent Activity</h2>
                        <p class="text-sm text-[var(--on-surface-variant)]">Latest API transactions across all keys</p>
                    </div>
                    <Button variant="ghost" size="sm">View All Logs</Button>
                </div>

                <div class="overflow-x-auto">
                    <table class="w-full">
                        <thead>
                            <tr class="bg-[var(--surface-container-low)]">
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Timestamp</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Key Name</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Action</th>
                                <th class="px-5 py-3 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Tokens</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each activityLog as entry, i}
                                <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                    <td class="px-5 py-3.5 text-sm text-[var(--on-surface-variant)] font-mono">{entry.timestamp}</td>
                                    <td class="px-5 py-3.5">
                                        <div class="flex items-center gap-2">
                                            <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 16px">vpn_key</span>
                                            <span class="text-sm font-medium text-[var(--on-surface)]">{entry.key}</span>
                                        </div>
                                    </td>
                                    <td class="px-5 py-3.5">
                                        <Badge variant={entry.action === 'Inference' ? 'info' : entry.action === 'Embedding' ? 'success' : 'neutral'} label={entry.action} />
                                    </td>
                                    <td class="px-5 py-3.5 text-sm text-[var(--on-surface)] text-right font-mono">{entry.tokens}</td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
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
