<script lang="ts">
    import { api } from '$lib/api';
    import type { Team } from '$lib/types';
    import { onMount } from 'svelte';
    import Button from '$lib/components/Button.svelte';
    import Input from '$lib/components/Input.svelte';
    import Modal from '$lib/components/Modal.svelte';
    import Card from '$lib/components/Card.svelte';

    let teams = $state<Team[]>([]);
    let loading = $state(true);
    let showCreate = $state(false);
    let newName = $state('');
    let newBudget = $state(10000);
    let createError = $state('');

    onMount(async () => {
        try {
            teams = await api.getTeams();
        } catch (e) {
            console.error('Failed to load teams', e);
        } finally {
            loading = false;
        }
    });

    function validateBudget(value: number): number {
        if (!Number.isFinite(value) || value < 0) return 0;
        return Math.round(value);
    }

    async function createTeam() {
        const name = newName.trim();
        if (!name) { createError = 'Team name is required'; return; }
        const budget = validateBudget(newBudget);
        if (budget < 0) { createError = 'Budget must be a non-negative number'; return; }
        createError = '';
        try {
            const team = await api.createTeam(name, budget);
            teams = [...teams, team];
            showCreate = false;
            newName = '';
            newBudget = 10000;
        } catch (e) {
            console.error('Failed to create team', e);
            createError = 'Failed to create team';
        }
    }
</script>

<div class="space-y-8">
    <div class="flex items-center justify-between">
        <div>
            <p class="text-xs uppercase tracking-[0.2em] text-[var(--on-surface-variant)] mb-1">Organization</p>
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">Teams</h1>
        </div>
        <Button onclick={() => showCreate = true}>
            <span class="material-symbols-outlined" style="font-size: 18px">add</span>
            Create Team
        </Button>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-64">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
        </div>
    {:else if teams.length === 0}
        <Card class="text-center py-12">
            <div class="space-y-3">
                <div class="w-16 h-16 rounded-2xl bg-[var(--surface-container)] flex items-center justify-center mx-auto">
                    <span class="material-symbols-outlined text-[var(--outline)]" style="font-size: 32px">group</span>
                </div>
                <p class="text-[var(--on-surface-variant)]">No teams yet</p>
                <Button size="sm" onclick={() => showCreate = true}>Create your first team</Button>
            </div>
        </Card>
    {:else}
        <Card padding="none">
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Name</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Budget</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Created</th>
                            <th class="px-5 py-3.5 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each teams as team, i}
                            <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="px-5 py-4">
                                    <div class="flex items-center gap-3">
                                        <div class="w-8 h-8 rounded-lg bg-[var(--surface-container)] flex items-center justify-center">
                                            <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">group</span>
                                        </div>
                                        <span class="text-sm font-medium text-[var(--on-surface)]">{team.name}</span>
                                    </div>
                                </td>
                                <td class="px-5 py-4 text-sm text-[var(--on-surface)] font-mono">${'$'}{(team.budget_cents / 100).toFixed(2)}</td>
                                <td class="px-5 py-4 text-sm text-[var(--on-surface-variant)]">{new Date(team.created_at).toLocaleDateString()}</td>
                                <td class="px-5 py-4 text-right">
                                    <Button href="/dashboard/teams/{team.id}" variant="ghost" size="sm">
                                        View
                                        <span class="material-symbols-outlined" style="font-size: 16px">chevron_right</span>
                                    </Button>
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
        </Card>
    {/if}
</div>

<Modal bind:open={showCreate}>
    <form class="p-6 space-y-6" onsubmit={(e) => { e.preventDefault(); createTeam(); }}>
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Create Team</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Set up a new organizational team.</p>
        </div>
        <Input label="Team Name" bind:value={newName} placeholder="e.g. Engineering Alpha" required />
        <Input label="Budget (cents)" type="number" value={String(newBudget)} oninput={(e) => { newBudget = Number((e.target as HTMLInputElement).value) || 0; }} placeholder="10000" />
        {#if createError}
            <div class="flex items-center gap-2 text-sm text-[var(--error)] bg-[var(--error-container)]/20 rounded-lg px-4 py-3">
                <span class="material-symbols-outlined" style="font-size: 18px">error</span>
                {createError}
            </div>
        {/if}
        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={() => { showCreate = false; createError = ''; }}>Cancel</Button>
            <Button type="submit">Create</Button>
        </div>
    </form>
</Modal>
