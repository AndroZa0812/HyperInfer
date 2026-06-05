<script lang="ts">
    import { api } from '$lib/api';
    import type { Deployment, CreateDeploymentRequest } from '$lib/types';
    import { onMount } from 'svelte';

    let deployments = $state<Deployment[]>([]);
    let loading = $state(true);
    let showCreate = $state(false);
    let editingId = $state<string | null>(null);
    let modelFilter = $state('gpt-4');
    let createError = $state('');

    let formData = $state<CreateDeploymentRequest>({
        name: '',
        provider: 'openai',
        model: 'gpt-4',
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
            resetForm();
        } catch (e) {
            console.error('Failed to update deployment', e);
            createError = 'Failed to update deployment';
        }
    }

    async function deleteDeployment(id: string) {
        if (!confirm('Are you sure you want to delete this deployment?')) return;
        try {
            await api.deleteDeployment(id);
            deployments = deployments.filter(d => d.id !== id);
        } catch (e) {
            console.error('Failed to delete deployment', e);
        }
    }

    function startEdit(deployment: Deployment) {
        editingId = deployment.id;
        formData = {
            name: deployment.name,
            provider: deployment.provider,
            model: deployment.model,
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
        <h1 class="text-2xl font-bold">Routing Deployments</h1>
        <div class="flex gap-2">
            <input
                type="text"
                placeholder="Filter by model..."
                class="px-3 py-2 border rounded-lg"
                bind:value={modelFilter}
                onkeydown={(e) => e.key === 'Enter' && loadDeployments()}
            />
            <button
                class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg"
                onclick={() => { showCreate = true; editingId = null; resetForm(); }}
            >
                Add Deployment
            </button>
        </div>
    </div>

    {#if loading}
        <div class="text-center py-8">Loading...</div>
    {:else if deployments.length === 0}
        <div class="text-center py-8 text-gray-500">No deployments found for model "{modelFilter}"</div>
    {:else}
        <div class="bg-[var(--bg-primary)] rounded-lg border overflow-hidden">
            <table class="w-full">
                <thead>
                    <tr class="border-b bg-[var(--bg-secondary)]">
                        <th class="text-left p-3">Name</th>
                        <th class="text-left p-3">Provider</th>
                        <th class="text-left p-3">Model</th>
                        <th class="text-left p-3">Base URL</th>
                        <th class="text-left p-3">Weight</th>
                        <th class="text-left p-3">Status</th>
                        <th class="text-left p-3">Actions</th>
                    </tr>
                </thead>
                <tbody>
                    {#each deployments as deployment}
                        <tr class="border-b hover:bg-[var(--bg-secondary)]">
                            <td class="p-3">{deployment.name}</td>
                            <td class="p-3">{deployment.provider}</td>
                            <td class="p-3">{deployment.model}</td>
                            <td class="p-3 text-sm truncate max-w-[200px]">{deployment.base_url}</td>
                            <td class="p-3">{deployment.weight}</td>
                            <td class="p-3">
                                <span class="px-2 py-1 rounded text-xs {deployment.is_active ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}">
                                    {deployment.is_active ? 'Active' : 'Inactive'}
                                </span>
                            </td>
                            <td class="p-3">
                                <button class="text-[var(--accent)] mr-2" onclick={() => startEdit(deployment)}>Edit</button>
                                <button class="text-red-500" onclick={() => deleteDeployment(deployment.id)}>Delete</button>
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {/if}

    {#if showCreate}
        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div class="bg-[var(--bg-primary)] rounded-lg p-6 w-full max-w-md">
                <h2 class="text-xl font-bold mb-4">{editingId ? 'Edit Deployment' : 'Create Deployment'}</h2>
                {#if createError}
                    <div class="text-red-500 text-sm mb-2">{createError}</div>
                {/if}
                <div class="space-y-4">
                    <div>
                        <label class="block text-sm mb-1">Name</label>
                        <input class="w-full px-3 py-2 border rounded-lg" bind:value={formData.name} />
                    </div>
                    <div>
                        <label class="block text-sm mb-1">Provider</label>
                        <select class="w-full px-3 py-2 border rounded-lg" bind:value={formData.provider}>
                            <option value="openai">OpenAI</option>
                            <option value="anthropic">Anthropic</option>
                        </select>
                    </div>
                    <div>
                        <label class="block text-sm mb-1">Model</label>
                        <input class="w-full px-3 py-2 border rounded-lg" bind:value={formData.model} />
                    </div>
                    <div>
                        <label class="block text-sm mb-1">Base URL</label>
                        <input class="w-full px-3 py-2 border rounded-lg" bind:value={formData.base_url} />
                    </div>
                    <div>
                        <label class="block text-sm mb-1">Weight</label>
                        <input type="number" class="w-full px-3 py-2 border rounded-lg" bind:value={formData.weight} min="0" />
                    </div>
                    <div>
                        <label class="block text-sm mb-1">Priority</label>
                        <input type="number" class="w-full px-3 py-2 border rounded-lg" bind:value={formData.priority} min="0" />
                    </div>
                    <div class="flex items-center gap-2">
                        <input type="checkbox" id="is_active" bind:checked={formData.is_active} />
                        <label for="is_active">Active</label>
                    </div>
                </div>
                <div class="flex justify-end gap-2 mt-6">
                    <button class="px-4 py-2 border rounded-lg" onclick={cancelEdit}>Cancel</button>
                    <button class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg" onclick={editingId ? updateDeployment : createDeployment}>
                        {editingId ? 'Update' : 'Create'}
                    </button>
                </div>
            </div>
        </div>
    {/if}
</div>
