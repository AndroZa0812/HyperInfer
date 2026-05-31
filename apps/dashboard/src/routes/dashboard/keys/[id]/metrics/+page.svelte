<script lang="ts">
    import { page } from '$app/stores';
    import { api } from '$lib/api';
    import type { ApiKey } from '$lib/types';
    import { onMount } from 'svelte';
    import StatCard from '$lib/components/StatCard.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import Breadcrumbs from '$lib/components/Breadcrumbs.svelte';

    let keyData = $state<ApiKey | null>(null);
    let loading = $state(true);

    let keyId = $derived($page.params.id);

    onMount(async () => {
        if (!keyId) { loading = false; return; }
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
    <div class="flex items-center justify-center h-64">
        <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
    </div>
{:else if keyData}
    <div class="space-y-8">
        <Breadcrumbs items={[
            { label: 'Keys', href: '/dashboard/keys' },
            { label: keyData.name }
        ]} />

        <div class="flex items-center gap-3">
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">{keyData.name}</h1>
            <Badge variant={keyData.is_active ? 'success' : 'error'} label={keyData.is_active ? 'Active' : 'Revoked'} />
        </div>

        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
            <StatCard icon="vpn_key" label="Status" value={keyData.is_active ? 'Active' : 'Revoked'} />
            <StatCard icon="calendar_today" label="Created" value={new Date(keyData.created_at).toLocaleDateString()} />
            <StatCard icon="schedule" label="Last Used" value={keyData.last_used_at ? new Date(keyData.last_used_at).toLocaleDateString() : 'Never'} />
        </div>
    </div>
{:else}
    <p class="text-[var(--on-surface-variant)]">Key not found</p>
{/if}
