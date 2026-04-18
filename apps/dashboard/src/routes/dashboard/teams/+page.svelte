<script lang="ts">
    import { api } from '$lib/api';
    import type { Team } from '$lib/types';
    import { onMount } from 'svelte';

    let teams: Team[] = [];
    let loading = true;
    let showCreate = false;
    let newName = '';
    let newBudget = 10000;

    onMount(async () => {
        try {
            teams = await api.getTeams();
        } catch (e) {
            console.error('Failed to load teams', e);
        } finally {
            loading = false;
        }
    });

    async function createTeam() {
        try {
            const team = await api.createTeam(newName, newBudget);
            teams = [...teams, team];
            showCreate = false;
            newName = '';
            newBudget = 10000;
        } catch (e) {
            console.error('Failed to create team', e);
        }
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold">Teams</h1>
        <button
            class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg"
            on:click={() => showCreate = true}
        >
            Create Team
        </button>
    </div>

    {#if loading}
        <p>Loading...</p>
    {:else if teams.length === 0}
        <p class="text-gray-500">No teams yet</p>
    {:else}
        <div class="bg-[var(--bg-primary)] rounded-xl overflow-hidden">
            <table class="w-full">
                <thead class="bg-[var(--bg-secondary)]">
                    <tr>
                        <th class="px-4 py-3 text-left">Name</th>
                        <th class="px-4 py-3 text-left">Budget</th>
                        <th class="px-4 py-3 text-left">Created</th>
                        <th class="px-4 py-3"></th>
                    </tr>
                </thead>
                <tbody>
                    {#each teams as team}
                        <tr class="border-t border-[var(--bg-secondary)]">
                            <td class="px-4 py-3">{team.name}</td>
                            <td class="px-4 py-3">${(team.budget_cents / 100).toFixed(2)}</td>
                            <td class="px-4 py-3">{new Date(team.created_at).toLocaleDateString()}</td>
                            <td class="px-4 py-3">
                                <a href="/dashboard/teams/{team.id}" class="text-[var(--accent)] hover:underline">View</a>
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {/if}
</div>

{#if showCreate}
    <div class="fixed inset-0 bg-black/50 flex items-center justify-center">
        <div class="bg-[var(--bg-primary)] p-6 rounded-xl w-96">
            <h2 class="text-lg font-semibold mb-4">Create Team</h2>
            <input
                bind:value={newName}
                placeholder="Team name"
                class="w-full px-4 py-2 border rounded-lg mb-4"
            />
            <input
                type="number"
                bind:value={newBudget}
                placeholder="Budget (cents)"
                class="w-full px-4 py-2 border rounded-lg mb-4"
            />
            <div class="flex gap-2 justify-end">
                <button class="px-4 py-2" on:click={() => showCreate = false}>Cancel</button>
                <button class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg" on:click={createTeam}>
                    Create
                </button>
            </div>
        </div>
    </div>
{/if}
