<script lang="ts">
    import { api } from '#lib/api';
    import type { McpTeamPolicy, McpToolDefinition, McpUpstream, Team } from '#lib/types';
    import { onMount } from 'svelte';
    import Card from '@hyperinfer/ui/Card.svelte';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';
    import Select from '@hyperinfer/ui/Select.svelte';
    import Modal from '@hyperinfer/ui/Modal.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';

    let upstreams = $state<McpUpstream[]>([]);
    let tools = $state<McpToolDefinition[]>([]);
    let catalogMetaTools = $state<McpToolDefinition[]>([]);
    let teams = $state<Team[]>([]);
    let loading = $state(true);
    let refreshingTools = $state(false);

    // Add Upstream modal state
    let showAddModal = $state(false);
    let newName = $state('');
    let newTransport = $state<'streamable_http' | 'sse'>('streamable_http');
    let newUrl = $state('');
    let newAuthHeader = $state('');
    let newEnabled = $state(true);
    let upstreamError = $state('');
    let pendingRemoveName = $state<string | null>(null);
    let showRemoveConfirm = $state(false);
    let removeError = $state('');

    // Per-team policy state
    let selectedTeamId = $state('');
    let allowPatternsText = $state('');
    let denyPatternsText = $state('');
    let catalogMode = $state(false);
    let savingPolicy = $state(false);
    let policyStatus = $state('');
    let policyError = $state('');

    onMount(async () => {
        loading = true;
        try {
            const [upstreamsRes, toolsRes, teamsRes] = await Promise.allSettled([
                api.getMcpUpstreams(),
                api.getMcpTools(),
                api.getTeams(),
            ]);
            if (upstreamsRes.status === 'fulfilled') upstreams = upstreamsRes.value;
            if (toolsRes.status === 'fulfilled') {
                tools = toolsRes.value.tools;
                catalogMetaTools = toolsRes.value.catalog_meta_tools;
            }
            if (teamsRes.status === 'fulfilled') {
                teams = teamsRes.value;
                if (teams.length > 0) {
                    selectedTeamId = teams[0].id;
                    await loadTeamPolicy(teams[0].id);
                }
            }
        } finally {
            loading = false;
        }
    });

    async function loadTeamPolicy(teamId: string) {
        if (!teamId) return;
        policyStatus = '';
        policyError = '';
        try {
            const policy = await api.getMcpTeamPolicy(teamId);
            allowPatternsText = policy.allow_patterns.join('\n');
            denyPatternsText = policy.deny_patterns.join('\n');
            catalogMode = policy.catalog_mode;
        } catch (e) {
            console.error('Failed to load MCP team policy', e);
            policyError = 'Failed to load team policy';
        }
    }

    async function saveTeamPolicy() {
        if (!selectedTeamId) return;
        savingPolicy = true;
        policyStatus = '';
        policyError = '';
        const policy: McpTeamPolicy = {
            allow_patterns: allowPatternsText.split('\n').map((s) => s.trim()).filter(Boolean),
            deny_patterns: denyPatternsText.split('\n').map((s) => s.trim()).filter(Boolean),
            catalog_mode: catalogMode,
        };
        try {
            const saved = await api.updateMcpTeamPolicy(selectedTeamId, policy);
            allowPatternsText = saved.allow_patterns.join('\n');
            denyPatternsText = saved.deny_patterns.join('\n');
            catalogMode = saved.catalog_mode;
            policyStatus = 'Policy saved';
        } catch (e) {
            console.error('Failed to save MCP team policy', e);
            policyError = 'Invalid regex pattern or failed to save policy';
        } finally {
            savingPolicy = false;
        }
    }

    async function addUpstream() {
        upstreamError = '';
        if (!newName.trim() || !newUrl.trim()) {
            upstreamError = 'Name and URL are required';
            return;
        }
        const headers: Record<string, string> = {};
        if (newAuthHeader.trim()) {
            headers['Authorization'] = newAuthHeader.trim();
        }
        try {
            const created = await api.registerMcpUpstream({
                name: newName.trim(),
                transport: newTransport,
                url: newUrl.trim(),
                headers,
                enabled: newEnabled,
            });
            upstreams = [...upstreams.filter((u) => u.name !== created.name), created].sort((a, b) =>
                a.name.localeCompare(b.name)
            );
            showAddModal = false;
            newName = '';
            newUrl = '';
            newAuthHeader = '';
            newEnabled = true;
            await refreshTools();
        } catch (e) {
            console.error('Failed to register upstream', e);
            upstreamError = 'Failed to register upstream server. Check name (alphanumeric/-/_) and URL (http/https).';
        }
    }

    function requestRemoveUpstream(name: string) {
        pendingRemoveName = name;
        removeError = '';
        showRemoveConfirm = true;
    }

    async function confirmRemoveUpstream() {
        if (!pendingRemoveName) return;
        const target = pendingRemoveName;
        removeError = '';
        try {
            await api.removeMcpUpstream(target);
            upstreams = upstreams.filter((u) => u.name !== target);
            showRemoveConfirm = false;
            pendingRemoveName = null;
            await refreshTools();
        } catch (e) {
            console.error('Failed to remove upstream', e);
            removeError = 'Failed to remove upstream server';
        }
    }

    async function refreshTools() {
        refreshingTools = true;
        try {
            const res = await api.getMcpTools();
            tools = res.tools;
            catalogMetaTools = res.catalog_meta_tools;
        } catch (e) {
            console.error('Failed to refresh MCP tools', e);
        } finally {
            refreshingTools = false;
        }
    }
</script>

<div class="space-y-8">
    <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-[220px] flex-1">
            <h1 class="text-2xl font-bold text-[var(--on-surface)]">MCP Gateway</h1>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                Manage upstream MCP servers, inspect discovered tools, and configure per-team regex allow/deny policies.
            </p>
        </div>
        <Button class="shrink-0" onclick={() => { upstreamError = ''; showAddModal = true; }}>
            <span class="material-symbols-outlined" style="font-size: 18px">add</span>
            Add Upstream Server
        </Button>
    </div>

    {#if loading}
        <div class="flex items-center justify-center h-48">
            <span class="material-symbols-outlined animate-spin text-[var(--primary)]" style="font-size: 28px">
                progress_activity
            </span>
        </div>
    {:else}
        <!-- Upstream MCP Servers -->
        <Card padding="none">
            <div class="px-6 py-4 border-b border-[var(--ghost-border)] flex flex-wrap items-start justify-between gap-3">
                <div class="min-w-[200px] flex-1">
                    <h2 class="text-base font-semibold text-[var(--on-surface)]">Upstream MCP Servers</h2>
                    <p class="text-xs text-[var(--on-surface-variant)]">
                        Tools are namespaced as <code class="font-mono">{'{server}__{tool}'}</code>. Auth headers are injected server-side and never returned to clients.
                    </p>
                </div>
                <Badge variant="info" label={`${upstreams.length} configured`} />
            </div>

            {#if upstreams.length === 0}
                <div class="p-8 text-center text-sm text-[var(--on-surface-variant)]">
                    No upstream MCP servers registered yet. Click <strong>Add Upstream Server</strong> to connect one.
                </div>
            {:else}
                <div class="overflow-x-auto">
                    <table class="w-full">
                        <thead>
                            <tr class="bg-[var(--surface-container-low)]">
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Name</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Transport</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Endpoint URL</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Injected Headers</th>
                                <th class="px-5 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Status</th>
                                <th class="px-5 py-3 text-right text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Actions</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each upstreams as server, i}
                                <tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''}">
                                    <td class="px-5 py-3.5 text-sm font-mono font-medium text-[var(--on-surface)]">{server.name}</td>
                                    <td class="px-5 py-3.5 text-xs text-[var(--on-surface-variant)]">{server.transport}</td>
                                    <td class="px-5 py-3.5 text-xs font-mono text-[var(--on-surface-variant)]">{server.url}</td>
                                    <td class="px-5 py-3.5 text-xs text-[var(--on-surface-variant)]">
                                        {server.header_keys.length > 0 ? server.header_keys.join(', ') : 'None'}
                                    </td>
                                    <td class="px-5 py-3.5">
                                        <Badge variant={server.enabled ? 'success' : 'neutral'} label={server.enabled ? 'Enabled' : 'Disabled'} />
                                    </td>
                                    <td class="px-5 py-3.5 text-right">
                                        <Button variant="ghost" size="sm" onclick={() => requestRemoveUpstream(server.name)}>
                                            Remove
                                        </Button>
                                    </td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}
        </Card>

        <!-- Per-Team MCP Tool Policies -->
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <Card>
                <div class="space-y-5">
                    <div class="flex items-center justify-between gap-4">
                        <div>
                            <h2 class="text-base font-semibold text-[var(--on-surface)]">Per-Team Tool Policy</h2>
                            <p class="text-xs text-[var(--on-surface-variant)]">
                                Deny patterns always take precedence over allow patterns. Empty allow list permits all non-denied tools.
                            </p>
                        </div>
                    </div>

                    <Select
                        id="mcp-policy-team"
                        label="Team"
                        bind:value={selectedTeamId}
                        onchange={(v) => loadTeamPolicy(v)}
                        options={teams.map((t) => ({ value: t.id, label: t.name }))}
                    />

                    <div>
                        <label for="mcp-allow-patterns" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                            Allow Patterns (one regex per line)
                        </label>
                        <textarea
                            id="mcp-allow-patterns"
                            bind:value={allowPatternsText}
                            rows="3"
                            placeholder="^github__.*"
                            class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm font-mono"
                        ></textarea>
                    </div>

                    <div>
                        <label for="mcp-deny-patterns" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                            Deny Patterns (one regex per line)
                        </label>
                        <textarea
                            id="mcp-deny-patterns"
                            bind:value={denyPatternsText}
                            rows="3"
                            placeholder="github__delete_.*"
                            class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm font-mono"
                        ></textarea>
                    </div>

                    <label class="flex items-center gap-3 cursor-pointer">
                        <input type="checkbox" bind:checked={catalogMode} class="rounded" />
                        <div>
                            <span class="text-sm font-medium text-[var(--on-surface)]">Catalog / Code Mode</span>
                            <p class="text-xs text-[var(--on-surface-variant)]">
                                Return only compact meta-tools (<code>mcp_search_tools</code>, <code>mcp_call_tool</code>) in <code>tools/list</code> to save context tokens.
                            </p>
                        </div>
                    </label>

                    {#if policyError}
                        <p class="text-xs text-[var(--error)]">{policyError}</p>
                    {/if}
                    {#if policyStatus}
                        <p class="text-xs text-[var(--success)]">{policyStatus}</p>
                    {/if}

                    <div class="flex justify-end">
                        <Button size="sm" disabled={savingPolicy || !selectedTeamId} onclick={saveTeamPolicy}>
                            {savingPolicy ? 'Saving...' : 'Save Policy'}
                        </Button>
                    </div>
                </div>
            </Card>

            <!-- Discovered Tools & Catalog Meta-Tools -->
            <Card>
                <div class="space-y-4">
                    <div class="flex flex-wrap items-start justify-between gap-3">
                        <div class="min-w-[180px] flex-1">
                            <h2 class="text-base font-semibold text-[var(--on-surface)]">Discovered Tools</h2>
                            <p class="text-xs text-[var(--on-surface-variant)]">
                                Active tools available across local host and enabled upstreams.
                            </p>
                        </div>
                        <Button variant="secondary" size="sm" class="shrink-0" disabled={refreshingTools} onclick={refreshTools}>
                            {refreshingTools ? 'Refreshing...' : 'Refresh'}
                        </Button>
                    </div>

                    {#if tools.length === 0}
                        <p class="text-sm text-[var(--on-surface-variant)] py-4">
                            No upstream or local tools discovered yet. Register an enabled Streamable HTTP upstream to discover tools.
                        </p>
                    {:else}
                        <div class="space-y-2 max-h-64 overflow-y-auto">
                            {#each tools as tool}
                                <div class="p-3 rounded-lg bg-[var(--surface-container-low)]">
                                    <p class="text-xs font-mono font-semibold text-[var(--on-surface)]">{tool.name}</p>
                                    <p class="text-xs text-[var(--on-surface-variant)] mt-0.5">{tool.description}</p>
                                </div>
                            {/each}
                        </div>
                    {/if}

                    <div class="pt-3 border-t border-[var(--ghost-border)] space-y-2">
                        <p class="text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">
                            Catalog Mode Meta-Tools
                        </p>
                        {#each catalogMetaTools as meta}
                            <div class="p-2.5 rounded-lg bg-[var(--surface-container-low)]/60">
                                <p class="text-xs font-mono text-[var(--primary)]">{meta.name}</p>
                                <p class="text-xs text-[var(--on-surface-variant)]">{meta.description}</p>
                            </div>
                        {/each}
                    </div>
                </div>
            </Card>
        </div>
    {/if}
</div>

<Modal bind:open={showAddModal}>
    <form class="p-6 space-y-5" onsubmit={(e) => { e.preventDefault(); addUpstream(); }}>
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Add Upstream MCP Server</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                Connect an upstream MCP endpoint. Header values starting with <code>$</code> resolve from server environment variables.
            </p>
        </div>

        <Input label="Server Name" bind:value={newName} placeholder="e.g. github" required />

        <Select
            id="mcp-transport"
            label="Transport"
            bind:value={newTransport}
            options={[
                { value: 'streamable_http', label: 'Streamable HTTP' },
                { value: 'sse', label: 'SSE' },
            ]}
        />

        <Input label="Endpoint URL" bind:value={newUrl} placeholder="https://mcp.example.com/mcp" required />
        <Input label="Authorization Header (optional)" bind:value={newAuthHeader} placeholder="e.g. $GITHUB_MCP_TOKEN" />

        <label class="flex items-center gap-2 text-sm text-[var(--on-surface)] cursor-pointer">
            <input type="checkbox" bind:checked={newEnabled} />
            Enabled
        </label>

        {#if upstreamError}
            <p class="text-sm text-[var(--error)]">{upstreamError}</p>
        {/if}

        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={() => showAddModal = false}>Cancel</Button>
            <Button type="submit">Add Server</Button>
        </div>
    </form>
</Modal>

<Modal bind:open={showRemoveConfirm}>
    <div class="p-6 space-y-5">
        <div>
            <h2 class="text-xl font-semibold text-[var(--on-surface)]">Remove Upstream Server</h2>
            <p class="text-sm text-[var(--on-surface-variant)] mt-1">
                Are you sure you want to disconnect <strong class="font-mono text-[var(--on-surface)]">{pendingRemoveName}</strong> from the MCP gateway?
            </p>
        </div>
        {#if removeError}
            <p class="text-sm text-[var(--error)]">{removeError}</p>
        {/if}
        <div class="flex gap-3 justify-end">
            <Button variant="ghost" onclick={() => { showRemoveConfirm = false; pendingRemoveName = null; }}>Cancel</Button>
            <Button class="bg-[var(--error)] text-white hover:opacity-90" onclick={confirmRemoveUpstream}>Remove</Button>
        </div>
    </div>
</Modal>
