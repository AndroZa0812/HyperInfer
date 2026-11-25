<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { ApiKey, CreateApiKeyResponse, UsageData } from '#lib/types';
    import Button from '#lib/components/Button.svelte';
    import Input from '#lib/components/Input.svelte';
    import Modal from '#lib/components/Modal.svelte';
    import Badge from '#lib/components/Badge.svelte';
    import StatCard from '#lib/components/StatCard.svelte';
    import Card from '#lib/components/Card.svelte';

    let keys = $state<ApiKey[]>([]);
    let loading = $state(true);
    let loadError = $state<string | null>(null);
    let showCreate = $state(false);
    let newKeyName = $state('');
    let searchQuery = $state('');
    let statusFilter = $state('all');
    let usage = $state<UsageData | null>(null);
    let showSecret = $state(false);
    let createdKey = $state<CreateApiKeyResponse | null>(null);
    let copied = $state(false);

    let filteredKeys = $derived(
        keys.filter(k => {
            const matchesSearch = !searchQuery || (k.name ?? '').toLowerCase().includes(searchQuery.toLowerCase());
            const matchesStatus = statusFilter === 'all' ||
                (statusFilter === 'active' && k.is_active) ||
                (statusFilter === 'revoked' && !k.is_active);
            return matchesSearch && matchesStatus;
        })
    );

    $effect(() => {
        if (!$auth.loading && $auth.user?.team_id) {
            loading = true;
            loadError = null;
            Promise.allSettled([
                api.getKeys($auth.user!.team_id),
                api.getUsage($auth.user!.team_id)
            ])
                .then(([keysResult, usageResult]) => {
                    if (keysResult.status === 'fulfilled') {
                        keys = keysResult.value;
                    } else {
                        console.error('Failed to load keys', keysResult.reason);
                        loadError = 'Failed to load API keys. Please try again.';
                    }
                    if (usageResult.status === 'fulfilled') {
                        usage = usageResult.value;
                    } else {
                        console.error('Failed to load usage summary', usageResult.reason);
                    }
                })
                .finally(() => { loading = false; });
        }
    });

    async function createKey() {
        if (!$auth.user?.team_id) return;
        try {
            const response = await api.createKey($auth.user!.team_id, newKeyName);
            const { key: secret, ...keyMetadata } = response;
            keys = [...keys, keyMetadata];
            showCreate = false;
            newKeyName = '';
            createdKey = { ...keyMetadata, key: secret };
            showSecret = true;
            copied = false;
        } catch (e) {
            console.error('Failed to create key', e);
        }
    }

    function copyKey() {
        if (!createdKey) return;
        navigator.clipboard?.writeText(createdKey.key).then(() => {
            copied = true;
            setTimeout(() => { copied = false; }, 2000);
        }).catch(() => {});
    }

    function closeSecretModal() {
        showSecret = false;
        createdKey = null;
        copied = false;
    }

    async function revokeKey(keyId: string) {
        try {
            const updated = await api.revokeKey(keyId);
            keys = keys.map(k => k.id === keyId ? updated : k);
        } catch (e) {
            console.error('Failed to revoke key', e);
        }
    }
</script>

<div class="space-y-8">
    <div class="flex items-center justify-between">
        <div>
            <p class="text-xs uppercase tracking-[0.2em] text-[var(--on-surface-variant)] mb-1">Security Infrastructure</p>
            <h1 class="text-3xl font-bold text-[var(--on-surface)]">API Keys</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Manage authentication credentials for your production and development clusters.</p>
        </div>
        <Button onclick={() => showCreate = true}>
            <span class="material-symbols-outlined" style="font-size: 18px">add</span>
            Create New Key
        </Button>
    </div>

    <Card padding="none">
        <div class="p-4 flex items-center gap-4">
            <div class="flex-1 relative">
                <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-[var(--outline)]" style="font-size: 20px">search</span>
                <input
                    bind:value={searchQuery}
                    placeholder="Search keys..."
                    class="w-full pl-10 pr-4 py-2.5 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors text-sm"
                />
            </div>
            <div class="flex gap-1 bg-[var(--surface-container-low)] rounded-lg p-1">
                {#each [{l:'All Status',v:'all'},{l:'Active',v:'active'},{l:'Revoked',v:'revoked'}] as tab}
                    <button
                        class="px-3 py-1.5 text-xs font-medium rounded-md transition-all cursor-pointer {statusFilter === tab.v ? 'bg-[var(--surface-container-lowest)] text-[var(--primary)] shadow-sm' : 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'}"
                        onclick={() => statusFilter = tab.v}
                    >{tab.l}</button>
                {/each}
            </div>
        </div>

        {#if loading}
            <div class="flex items-center justify-center h-32">
                <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 24px">progress_activity</span>
            </div>
        {:else if loadError}
            <div class="p-8 text-center text-[var(--error)] text-sm">
                {loadError}
            </div>
        {:else if filteredKeys.length === 0}
            <div class="p-8 text-center text-[var(--on-surface-variant)]">
                {searchQuery || statusFilter !== 'all' ? 'No matching keys found' : 'No API keys yet'}
            </div>
        {:else}
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Key Name</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Prefix</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Created At</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Last Used</th>
                            <th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="px-5 py-3.5 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each filteredKeys as key, i}
                            <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="px-5 py-4">
                                    <div class="flex items-center gap-2">
                                        <span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 18px">vpn_key</span>
                                        <span class="text-sm font-medium text-[var(--on-surface)]">{key.name || 'Unnamed Key'}</span>
                                    </div>
                                </td>
                                <td class="px-5 py-4 text-sm font-mono text-[var(--on-surface-variant)]">{key.prefix ? `${key.prefix}...` : `${key.id.slice(0, 8)}...`}</td>
                                <td class="px-5 py-4 text-sm text-[var(--on-surface-variant)]">{new Date(key.created_at).toLocaleDateString()}</td>
                                <td class="px-5 py-4 text-sm text-[var(--on-surface-variant)]">{key.last_used_at ? new Date(key.last_used_at).toLocaleDateString() : 'Never'}</td>
                                <td class="px-5 py-4">
                                    <Badge variant={key.is_active ? 'success' : 'error'} label={key.is_active ? 'Active' : 'Revoked'} />
                                </td>
                                <td class="px-5 py-4 text-right">
                                    {#if key.is_active}
                                        <div class="flex items-center justify-end gap-1">
                                            <a href="/dashboard/keys/{key.id}/metrics" class="p-1.5 rounded-md hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--primary)] transition-all" title="Metrics">
                                                <span class="material-symbols-outlined" style="font-size: 18px">monitoring</span>
                                            </a>
                                            <button class="p-1.5 rounded-md hover:bg-[var(--error)]/10 text-[var(--on-surface-variant)] hover:text-[var(--error)] transition-all cursor-pointer" title="Revoke" onclick={() => revokeKey(key.id)}>
                                                <span class="material-symbols-outlined" style="font-size: 18px">block</span>
                                            </button>
                                        </div>
                                    {:else}
                                        <span class="text-xs text-[var(--outline)]">No Actions Available</span>
                                    {/if}
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
            <div class="px-5 py-3 flex items-center justify-between border-t border-[var(--ghost-border)]">
                <span class="text-xs text-[var(--on-surface-variant)]">Showing 1-{filteredKeys.length} of {keys.length} keys</span>
            </div>
        {/if}
    </Card>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <StatCard icon="speed" label="Total Requests" value={usage ? usage.total_requests.toLocaleString() : '0'} />
        <StatCard icon="security" label="Total Tokens" value={usage ? (usage.total_input_tokens + usage.total_output_tokens).toLocaleString() : '0'} />
        <StatCard icon="timer" label="Average Latency" value={usage ? `${Math.round(usage.avg_response_time_ms)}ms` : '0ms'} />
    </div>
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

<Modal bind:open={showSecret} onclose={closeSecretModal}>
    <div class="p-6 space-y-6">
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">API Key Created</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">Save this key now. It will only be shown once.</p>
        </div>
        <div class="bg-[var(--surface-container-low)] rounded-lg p-4">
            <p class="text-xs text-[var(--on-surface-variant)] mb-2">Secret Key</p>
            <p class="text-sm font-mono text-[var(--on-surface)] break-all">{createdKey?.key}</p>
        </div>
        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={copyKey}>
                <span class="material-symbols-outlined" style="font-size: 18px">{copied ? 'check' : 'content_copy'}</span>
                {copied ? 'Copied!' : 'Copy Key'}
            </Button>
            <Button onclick={closeSecretModal}>Done</Button>
        </div>
    </div>
</Modal>
