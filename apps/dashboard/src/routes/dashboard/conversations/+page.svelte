<script lang="ts">
    import { api } from '$lib/api';
    import { onMount } from 'svelte';
    import Card from '$lib/components/Card.svelte';
    import Button from '$lib/components/Button.svelte';

    interface Conversation {
        id: string;
        title: string | null;
        updated_at: string;
        created_at: string;
    }

    let conversations = $state<Conversation[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);
    let searchQuery = $state('');
    let selectedId = $state<string | null>(null);

    let filteredConversations = $derived(
        conversations.filter(c =>
            !searchQuery || (c.title || 'Untitled').toLowerCase().includes(searchQuery.toLowerCase())
        )
    );

    onMount(async () => {
        try {
            conversations = await api.getConversations() as Conversation[];
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to load conversations';
        } finally {
            loading = false;
        }
    });

    function timeAgo(dateStr: string): string {
        const diff = Date.now() - new Date(dateStr).getTime();
        const mins = Math.floor(diff / 60000);
        if (mins < 60) return `${mins}m ago`;
        const hours = Math.floor(mins / 60);
        if (hours < 24) return `${hours}h ago`;
        const days = Math.floor(hours / 24);
        return days === 1 ? 'Yesterday' : `${days}d ago`;
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-3xl font-bold text-[var(--on-surface)]">Conversations</h1>
        <Button size="sm">
            <span class="material-symbols-outlined" style="font-size: 18px">add</span>
            New Session
        </Button>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-64">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 32px">progress_activity</span>
        </div>
    {:else if error}
        <div class="flex items-center gap-2 text-[var(--error)]">
            <span class="material-symbols-outlined">error</span>
            {error}
        </div>
    {:else}
        <div class="grid grid-cols-1 lg:grid-cols-[380px_1fr] gap-6 min-h-[600px]">
            <Card padding="none" class="overflow-hidden flex flex-col">
                <div class="p-4">
                    <div class="relative">
                        <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-[var(--outline)]" style="font-size: 20px">search</span>
                        <input
                            bind:value={searchQuery}
                            placeholder="Search conversations..."
                            class="w-full pl-10 pr-4 py-2.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors text-sm"
                        />
                    </div>
                </div>

                <div class="flex-1 overflow-auto">
                    {#if filteredConversations.length === 0}
                        <div class="p-8 text-center text-[var(--on-surface-variant)] text-sm">
                            {searchQuery ? 'No matching conversations' : 'No conversations yet'}
                        </div>
                    {:else}
                        {#each filteredConversations as conv}
                            <a
                                href="/dashboard/conversations/{conv.id}"
                                class="flex items-start gap-3 px-4 py-3.5 transition-all hover:bg-[var(--surface-container-low)] {selectedId === conv.id ? 'bg-[var(--surface-container-low)]' : ''}"
                                onclick={() => selectedId = conv.id}
                            >
                                <div class="w-8 h-8 rounded-lg bg-[var(--surface-container)] flex items-center justify-center shrink-0 mt-0.5">
                                    <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">forum</span>
                                </div>
                                <div class="flex-1 min-w-0">
                                    <div class="flex items-center justify-between gap-2">
                                        <span class="text-sm font-medium text-[var(--on-surface)] truncate">{conv.title || 'Untitled'}</span>
                                        <span class="text-[11px] text-[var(--on-surface-variant)] shrink-0">{timeAgo(conv.updated_at || conv.created_at)}</span>
                                    </div>
                                </div>
                            </a>
                        {/each}
                    {/if}
                </div>
            </Card>

            <Card class="flex items-center justify-center">
                <div class="text-center space-y-3">
                    <div class="w-16 h-16 rounded-2xl bg-[var(--surface-container)] flex items-center justify-center mx-auto">
                        <span class="material-symbols-outlined text-[var(--outline)]" style="font-size: 32px">forum</span>
                    </div>
                    <p class="text-[var(--on-surface-variant)]">Select a conversation to view details</p>
                    <p class="text-xs text-[var(--outline)]">Use CTRL+K to quickly search through conversation history</p>
                </div>
            </Card>
        </div>
    {/if}
</div>
