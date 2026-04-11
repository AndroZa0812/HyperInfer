<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { ApiKey } from '$lib/types';
    import { onMount } from 'svelte';

    let keys: ApiKey[] = [];
    let loading = true;
    let showCreate = false;
    let newKeyName = '';

    $: teamId = $page.params.id;

    onMount(async () => {
        if (teamId) {
            try {
                keys = await api.getKeys(teamId);
            } catch (e) {
                console.error('Failed to load keys', e);
            } finally {
                loading = false;
            }
        }
    });

    async function createKey() {
        try {
            const key = await api.createKey(teamId, newKeyName);
            keys = [...keys, key];
            showCreate = false;
            newKeyName = '';
        } catch (e) {
            console.error('Failed to create key', e);
        }
    }

    async function revokeKey(keyId: string) {
        try {
            await api.revokeKey(teamId, keyId);
            keys = keys.filter(k => k.id !== keyId);
        } catch (e) {
            console.error('Failed to revoke key', e);
        }
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold">API Keys</h1>
        <button
            class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg"
            on:click={() => showCreate = true}
        >
            Create Key
        </button>
    </div>

    {#if loading}
        <p>Loading...</p>
    {:else if keys.length === 0}
        <p class="text-gray-500">No API keys yet</p>
    {:else}
        <div class="bg-[var(--bg-primary)] rounded-xl overflow-hidden">
            <table class="w-full">
                <thead class="bg-[var(--bg-secondary)]">
                    <tr>
                        <th class="px-4 py-3 text-left">Name</th>
                        <th class="px-4 py-3 text-left">Key</th>
                        <th class="px-4 py-3 text-left">Created</th>
                        <th class="px-4 py-3 text-left">Status</th>
                        <th class="px-4 py-3 text-left">Actions</th>
                    </tr>
                </thead>
                <tbody>
                    {#each keys as key}
                        <tr class="border-t border-[var(--bg-secondary)]">
                            <td class="px-4 py-3">{key.name}</td>
                            <td class="px-4 py-3 font-mono">{key.prefix}...</td>
                            <td class="px-4 py-3">{new Date(key.created_at).toLocaleDateString()}</td>
                            <td class="px-4 py-3">
                                <span class="px-2 py-1 rounded-full text-xs {key.is_active ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}">
                                    {key.is_active ? 'Active' : 'Revoked'}
                                </span>
                            </td>
                            <td class="px-4 py-3">
                                <button
                                    class="text-red-500 text-sm"
                                    on:click={() => revokeKey(key.id)}
                                >
                                    Revoke
                                </button>
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
            <h2 class="text-lg font-semibold mb-4">Create API Key</h2>
            <input
                bind:value={newKeyName}
                placeholder="Key name"
                class="w-full px-4 py-2 border rounded-lg mb-4"
            />
            <div class="flex gap-2 justify-end">
                <button class="px-4 py-2" on:click={() => showCreate = false}>Cancel</button>
                <button class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg" on:click={createKey}>
                    Create
                </button>
            </div>
        </div>
    </div>
{/if}
