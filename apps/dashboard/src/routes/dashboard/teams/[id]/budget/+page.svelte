<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { Team } from '$lib/types';
    import { onMount } from 'svelte';

    let team: Team | null = null;
    let loading = true;

    $: teamId = $page.params.id;

    onMount(async () => {
        if (teamId) {
            try {
                team = await api.getTeam(teamId);
            } catch (e) {
                console.error('Failed to load team', e);
            } finally {
                loading = false;
            }
        }
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
                <div class="h-full bg-[var(--accent)]" style="width: 34%"></div>
            </div>
            <p class="text-sm text-gray-500 mt-2">${(team.budget_cents / 100 * 0.34).toFixed(2)} of ${(team.budget_cents / 100).toFixed(2)} used (34%)</p>
        </div>
    </div>
{:else}
    <p>Team not found</p>
{/if}
