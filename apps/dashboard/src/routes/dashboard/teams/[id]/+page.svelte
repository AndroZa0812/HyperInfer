<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { Team } from '$lib/types';

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
</script>

{#if loading}
    <p>Loading...</p>
{:else if error}
    <p class="text-red-500">{error}</p>
{:else if team}
    <div class="space-y-6">
        <div class="flex items-center justify-between">
            <h1 class="text-2xl font-bold">{team.name}</h1>
            <span class="px-3 py-1 rounded-full bg-green-100 text-green-800 text-sm">Active</span>
        </div>

        <div class="grid grid-cols-4 gap-4">
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Budget</p>
                <p class="text-2xl font-bold">${(team.budget_cents / 100).toFixed(2)}</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Created</p>
                <p class="text-2xl font-bold">{new Date(team.created_at).toLocaleDateString()}</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Usage</p>
                <p class="text-2xl font-bold">--</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Keys</p>
                <p class="text-2xl font-bold">--</p>
            </div>
        </div>

        <div class="flex gap-2">
            <a href="/dashboard/teams/{team.id}/keys" class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg">Manage Keys</a>
            <a href="/dashboard/teams/{team.id}/usage" class="px-4 py-2 bg-[var(--bg-secondary)] rounded-lg">Usage</a>
        </div>
    </div>
{:else}
    <p>Team not found</p>
{/if}