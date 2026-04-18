<script lang="ts">
    import { api } from '$lib/api';
    import { onMount } from 'svelte';

    let conversations = $state<any[]>([]);
    let loading = $state(true);

    onMount(async () => {
        try {
            conversations = await api.getConversations();
        } catch (e) {
            console.error('Failed to load conversations', e);
        } finally {
            loading = false;
        }
    });
</script>

<div class="space-y-6">
    <h1 class="text-2xl font-bold">Conversations</h1>

    {#if loading}
        <p>Loading...</p>
    {:else if conversations.length === 0}
        <p class="text-gray-500">No conversations yet</p>
    {:else}
        <div class="bg-[var(--bg-primary)] rounded-xl divide-y divide-[var(--bg-secondary)]">
            {#each conversations as conv}
                <a
                    href="/dashboard/conversations/{conv.id}"
                    class="p-4 flex items-center justify-between hover:bg-[var(--bg-secondary)]"
                >
                    <div>
                        <div class="font-medium">{conv.title || 'Untitled'}</div>
                        <div class="text-sm text-gray-500">{new Date(conv.updated_at || conv.created_at).toLocaleString()}</div>
                    </div>
                    <svg class="w-5 h-5 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                    </svg>
                </a>
            {/each}
        </div>
    {/if}
</div>