<script lang="ts">
    import { api } from '$lib/api';
    import type { Deployment, CreateDeploymentRequest } from '$lib/types';
    import { onMount } from 'svelte';
    import Card from '$lib/components/Card.svelte';
    import Modal from '$lib/components/Modal.svelte';
    import Button from '$lib/components/Button.svelte';
    import Badge from '$lib/components/Badge.svelte';
    import Input from '$lib/components/Input.svelte';

    let deployments = $state<Deployment[]>([]);
    let loading = $state(true);
    let showCreate = $state(false);
    let editingId = $state<string | null>(null);
    let modelFilter = $state('gpt-4');
    let createError = $state('');
    let pendingDeleteId = $state<string | null>(null);
    let showDeleteConfirm = $state(false);

    let formData = $state<CreateDeploymentRequest>({
        name: '',
        provider: 'openai',
        model: 'gpt-4',
        api_key_ref: '',
        base_url: 'https://api.openai.com/v1',
        is_active: true,
        weight: 1,
        priority: 0,
    });

    onMount(async () => {
        await loadDeployments();
    });

    async function loadDeployments() {
        loading = true;
        try {
            deployments = await api.getDeployments(modelFilter);
        } catch (e) {
            console.error('Failed to load deployments', e);
        } finally {
            loading = false;
        }
    }

    async function createDeployment() {
        createError = '';
        if (!formData.name.trim()) {
            createError = 'Name is required';
            return;
        }
        if (!formData.base_url.trim()) {
            createError = 'Base URL is required';
            return;
        }
        try {
            const deployment = await api.createDeployment(formData);
            deployments = [...deployments, deployment];
            showCreate = false;
            resetForm();
        } catch (e) {
            console.error('Failed to create deployment', e);
            createError = 'Failed to create deployment';
        }
    }

    async function updateDeployment() {
        if (!editingId) return;
        createError = '';
        try {
            const deployment = await api.updateDeployment(editingId, formData);
            deployments = deployments.map(d => d.id === editingId ? deployment : d);
            editingId = null;
            showCreate = false;
            resetForm();
        } catch (e) {
            console.error('Failed to update deployment', e);
            createError = 'Failed to update deployment';
        }
    }

    function confirmDelete(id: string) {
        pendingDeleteId = id;
        showDeleteConfirm = true;
    }

    async function deleteDeployment() {
        if (!pendingDeleteId) return;
        try {
            await api.deleteDeployment(pendingDeleteId);
            deployments = deployments.filter(d => d.id !== pendingDeleteId);
        } catch (e) {
            console.error('Failed to delete deployment', e);
        } finally {
            pendingDeleteId = null;
            showDeleteConfirm = false;
        }
    }

    function startEdit(deployment: Deployment) {
        editingId = deployment.id;
        formData = {
            name: deployment.name,
            provider: deployment.provider,
            model: deployment.model,
            api_key_ref: deployment.api_key_ref || '',
            base_url: deployment.base_url,
            is_active: deployment.is_active,
            weight: deployment.weight,
            priority: deployment.priority,
            max_tpm: deployment.max_tpm,
            max_rpm: deployment.max_rpm,
            cost_per_1k_input_tokens: deployment.cost_per_1k_input_tokens,
            cost_per_1k_output_tokens: deployment.cost_per_1k_output_tokens,
            sort_order: deployment.sort_order,
        };
        showCreate = true;
    }

    function resetForm() {
        formData = {
            name: '',
            provider: 'openai',
            model: 'gpt-4',
            api_key_ref: '',
            base_url: 'https://api.openai.com/v1',
            is_active: true,
            weight: 1,
            priority: 0,
        };
    }

    function cancelEdit() {
        editingId = null;
        showCreate = false;
        resetForm();
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-[var(--on-surface)]">Routing Deployments</h1>
        <div class="flex gap-2">
            <input
                type="text"
                placeholder="Filter by model..."
                class="px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                bind:value={modelFilter}
                onkeydown={(e) => e.key === 'Enter' && loadDeployments()}
            />
            <Button
                onclick={() => { showCreate = true; editingId = null; resetForm(); }}
            >
                Add Deployment
            </Button>
        </div>
    </div>

    {#if loading}
        <div class="text-center py-8 text-[var(--on-surface-variant)]">Loading...</div>
    {:else if deployments.length === 0}
        <div class="text-center py-8 text-[var(--on-surface-variant)]">No deployments found for model "{modelFilter}"</div>
    {:else}
        <Card variant="outlined" padding="none">
            <div class="overflow-x-auto">
                <table class="w-full">
                    <thead>
                        <tr class="bg-[var(--surface-container-low)]">
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Name</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Provider</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Model</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Base URL</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Weight</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                            <th class="text-left p-3 text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each deployments as deployment}
                            <tr class="border-t border-[var(--ghost-border)] hover:bg-[var(--surface-container-low)]/50 transition-colors">
                                <td class="p-3 text-sm text-[var(--on-surface)]">{deployment.name}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)]">{deployment.provider}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)]">{deployment.model}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)] truncate max-w-[200px]">{deployment.base_url}</td>
                                <td class="p-3 text-sm text-[var(--on-surface-variant)]">{deployment.weight}</td>
                                <td class="p-3">
                                    <Badge variant={deployment.is_active ? 'success' : 'error'} label={deployment.is_active ? 'Active' : 'Inactive'} />
                                </td>
                                <td class="p-3">
                                    <div class="flex gap-2">
                                        <Button variant="secondary" size="sm" onclick={() => startEdit(deployment)}>Edit</Button>
                                        <Button variant="ghost" size="sm" class="text-[var(--error)]" onclick={() => confirmDelete(deployment.id)}>Delete</Button>
                                    </div>
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
        </Card>
    {/if}

    <Modal bind:open={showCreate}>
        <div class="p-6">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-4">{editingId ? 'Edit Deployment' : 'Create Deployment'}</h2>
            {#if createError}
                <div class="text-[var(--error)] text-sm mb-4">{createError}</div>
            {/if}
            <div class="space-y-4">
                <Input id="dep-name" label="Name" bind:value={formData.name} />
                <div>
                    <label for="dep-provider" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Provider</label>
                    <select
                        id="dep-provider"
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                        bind:value={formData.provider}
                    >
                        <option value="openai">OpenAI</option>
                        <option value="anthropic">Anthropic</option>
                    </select>
                </div>
                <Input id="dep-model" label="Model" bind:value={formData.model} />
                <Input id="dep-base-url" label="Base URL" bind:value={formData.base_url} />
                <Input id="dep-api-key-ref" label="API Key Ref" bind:value={formData.api_key_ref} placeholder="env:OPENAI_API_KEY" />
                <div>
                    <label for="dep-weight" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Weight</label>
                    <input
                        id="dep-weight"
                        type="number"
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                        bind:value={formData.weight}
                        min="0"
                    />
                </div>
                <div>
                    <label for="dep-priority" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1.5">Priority</label>
                    <input
                        id="dep-priority"
                        type="number"
                        class="w-full px-4 py-3 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none transition-colors"
                        bind:value={formData.priority}
                        min="0"
                    />
                </div>
                <div class="flex items-center gap-2">
                    <input type="checkbox" id="dep-is-active" bind:checked={formData.is_active} class="rounded border-[var(--outline-variant)]" />
                    <label for="dep-is-active" class="text-sm text-[var(--on-surface)]">Active</label>
                </div>
            </div>
            <div class="flex justify-end gap-2 mt-6">
                <Button variant="ghost" onclick={cancelEdit}>Cancel</Button>
                <Button onclick={editingId ? updateDeployment : createDeployment}>
                    {editingId ? 'Update' : 'Create'}
                </Button>
            </div>
        </div>
    </Modal>

    <Modal bind:open={showDeleteConfirm} onclose={() => { pendingDeleteId = null; }}>
        <div class="p-6">
            <h2 class="text-xl font-bold text-[var(--on-surface)] mb-2">Delete Deployment</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mb-6">Are you sure you want to delete this deployment? This action cannot be undone.</p>
            <div class="flex justify-end gap-2">
                <Button variant="ghost" onclick={() => { showDeleteConfirm = false; pendingDeleteId = null; }}>Cancel</Button>
                <button
                    type="button"
                    class="inline-flex items-center justify-center font-medium rounded-lg px-5 py-2.5 text-sm bg-[var(--error)] text-[var(--on-error)] hover:opacity-90 transition-all cursor-pointer"
                    onclick={deleteDeployment}
                >
                    Delete
                </button>
            </div>
        </div>
    </Modal>
</div>