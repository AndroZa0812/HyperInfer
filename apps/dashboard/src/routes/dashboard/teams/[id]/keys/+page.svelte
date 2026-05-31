<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { ApiKey } from '$lib/types';
    import { onMount } from 'svelte';
    import Button from '$lib/components/Button.svelte';
    import Input from '$lib/components/Input.svelte';
    import Modal from '$lib/components/Modal.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import Card from '$lib/components/Card.svelte';
    import Breadcrumbs from '$lib/components/Breadcrumbs.svelte';

    let keys = $state<ApiKey[]>([]);
    let loading = $state(true);
    let showCreate = $state(false);
    let newKeyName = $state('');

    let teamId = $derived($page.params.id);

    async function loadKeys() {
        if (!teamId) return;
        loading = true;
        keys = [];
        try {
            keys = await api.getKeys(teamId);
        } catch (e) {
            console.error('Failed to load keys', e);
        } finally {
            loading = false;
        }
    }

    onMount(loadKeys);
    $effect(() => {
        if (teamId) loadKeys();
    });

    async function createKey() {
        if (!teamId) return;
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
        if (!teamId) return;
        try {
            const updated = await api.revokeKey(keyId);
            keys = keys.map(k => k.id === keyId ? updated : k);
        } catch (e) {
            console.error('Failed to revoke key', e);
        }
    }
</script>

<div class="space-y-8">
    <Breadcrumbs items={[
        { label: 'Teams', href: '/dashboard/teams' },
        { label: 'Team', href: `/dashboard/teams/${teamId}` },
        { label: 'API Keys' }
    ]} />

    <div class="flex items-center justify-between">
        <h1 class="text-3xl font-bold text-[var(--on-surface)]">API Keys</h1>
        <Button onclick={() => showCreate = true}>
            <span class="material-symbols-outlined" style="font-size: 18px">add</span>
            Create Key
        </Button>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-32">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 24px">progress_activity</span>
        </div>
    {:else if keys.length === 0}
        <Card class="text-center py-8">
            <p class="text-[var(--on-surface-variant)]">No API keys yet</p>
        </Card>
    {:else}
        <Card padding="none">
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Name</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Key</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Created</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="px-5 py-3.5 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each keys as key, i}
                            <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="px-5 py-4">
                                    <div class="flex items-center gap-2">
                                        <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">vpn_key</span>
                                        <span class="text-sm font-medium text-[var(--on-surface)]">{key.name}</span>
                                    </div>
                                </td>
                                <td class="px-5 py-4 text-sm font-mono text-[var(--on-surface-variant)]">{key.prefix}...</td>
                                <td class="px-5 py-4 text-sm text-[var(--on-surface-variant)]">{new Date(key.created_at).toLocaleDateString()}</td>
                                <td class="px-5 py-4">
                                    <Badge variant={key.is_active ? 'success' : 'error'} label={key.is_active ? 'Active' : 'Revoked'} />
                                </td>
                                <td class="px-5 py-4 text-right">
                                    {#if key.is_active}
                                        <div class="flex items-center justify-end gap-1">
                                            <button class="p-1.5 rounded-md hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--primary)] transition-all cursor-pointer">
                                                <span class="material-symbols-outlined" style="font-size: 18px">content_copy</span>
                                            </button>
                                            <button class="p-1.5 rounded-md hover:bg-[var(--error)]/10 text-[var(--on-surface-variant)] hover:text-[var(--error)] transition-all cursor-pointer" onclick={() => revokeKey(key.id)}>
                                                <span class="material-symbols-outlined" style="font-size: 18px">block</span>
                                            </button>
                                        </div>
                                    {:else}
                                        <span class="text-xs text-[var(--outline)]">No Actions</span>
                                    {/if}
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
    <form class="p-6 space-y-6" onsubmit={(e) => { e.preventDefault(); createKey(); }}>
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Create API Key</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Generate a new authentication credential.</p>
        </div>
        <Input label="Key Name" bind:value={newKeyName} placeholder="e.g. Production-Main" required />
        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={() => showCreate = false}>Cancel</Button>
            <Button type="submit">Create Key</Button>
        </div>
    </form>
</Modal>
