<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { ApiKey } from '$lib/types';
    import { onMount } from 'svelte';

    let keyData = $state<ApiKey | null>(null);
    let loading = $state(true);

    let keyId = $derived($page.params.id);

    onMount(async () => {
        try {
            keyData = await api.getKey(keyId);
        } catch (e) {
            console.error('Failed to load key metrics', e);
        } finally {
            loading = false;
        }
    });
</script>

{#if loading}
    <p>Loading...</p>
{:else if keyData}
    <div class="space-y-6">
        <h1 class="text-2xl font-bold">Key Metrics - {keyData.name}</h1>

        <div class="grid grid-cols-3 gap-4">
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Status</p>
                <p class="text-2xl font-bold">{keyData.is_active ? 'Active' : 'Revoked'}</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Created</p>
                <p class="text-2xl font-bold">{new Date(keyData.created_at).toLocaleDateString()}</p>
            </div>
            <div class="bg-[var(--bg-primary)] p-4 rounded-xl">
                <p class="text-sm text-gray-500">Last Used</p>
                <p class="text-2xl font-bold">{keyData.last_used_at ? new Date(keyData.last_used_at).toLocaleDateString() : 'Never'}</p>
            </div>
        </div>
    </div>
{:else}
    <p>Key not found</p>
{/if}