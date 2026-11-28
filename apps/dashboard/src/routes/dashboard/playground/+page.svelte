<script lang="ts">
    import { api } from '#lib/api';
    import { auth } from '#lib/stores/auth';
    import type { ModelAlias, OpenAiModelListItem } from '#lib/types';
    import { onMount } from 'svelte';
    import Card from '@hyperinfer/ui/Card.svelte';
    import Button from '@hyperinfer/ui/Button.svelte';
    import Input from '@hyperinfer/ui/Input.svelte';
    import Badge from '@hyperinfer/ui/Badge.svelte';

    type SdkTab = 'hyperinfer' | 'openai' | 'anthropic' | 'gemini' | 'responses';

    let model = $state('');
    let apiKey = $state('');
    let systemPrompt = $state('You are a helpful AI assistant.');
    let userPrompt = $state('');
    let temperature = $state<string | number>('0.7');
    let maxTokens = $state<string | number>('512');
    let aliases = $state<ModelAlias[]>([]);
    let deployedModels = $state<OpenAiModelListItem[]>([]);

    let sending = $state(false);
    let responseText = $state('');
    let responseError = $state('');
    let lastLatencyMs = $state<number | null>(null);
    let lastUsage = $state<{ prompt_tokens?: number; completion_tokens?: number; total_tokens?: number } | null>(null);

    let activeSdk = $state<SdkTab>('hyperinfer');
    let copiedSnippet = $state(false);
    let baseOrigin = $state('http://localhost:3000');
    let isAdmin = $derived($auth.user?.role === 'admin');

    onMount(async () => {
        if (typeof window !== 'undefined') {
            baseOrigin = window.location.origin;
        }
        const [modelsRes, aliasesRes] = await Promise.allSettled([
            api.getModels(),
            $auth.user?.team_id ? api.getModelAliases($auth.user.team_id) : Promise.resolve([]),
        ]);
        if (modelsRes.status === 'fulfilled') {
            deployedModels = modelsRes.value.data;
        }
        if (aliasesRes.status === 'fulfilled') {
            aliases = aliasesRes.value;
        }
        if (!model) {
            if (aliases.length > 0) {
                model = aliases[0].alias;
            } else if (deployedModels.length > 0) {
                model = deployedModels[0].id;
            }
        }
    });

    let keyDisplay = $derived(apiKey.trim() || '$HYPERINFER_API_KEY');

    let snippetCode = $derived.by(() => {
        const m = model.trim() || 'gpt-4o-mini';
        const k = keyDisplay;
        switch (activeSdk) {
            case 'hyperinfer':
                return `# Native HyperInfer Data Plane SDK (pip install hyperinfer)\nimport asyncio\nfrom hyperinfer import Client\n\nasync def main():\n    async with Client(\n        redis_url="redis://localhost:6379",\n        api_keys={"openai": "$OPENAI_API_KEY"},\n        virtual_key="${k}",\n    ) as client:\n        resp = await client.chat(\n            model="${m}",\n            messages=[{"role": "user", "content": "Hello from HyperInfer!"}],\n        )\n        print(resp["choices"][0]["message"]["content"])\n\nasyncio.run(main())`;
            case 'openai':
                return `from openai import OpenAI\n\nclient = OpenAI(\n    base_url="${baseOrigin}/v1",\n    api_key="${k}",\n)\nresp = client.chat.completions.create(\n    model="${m}",\n    messages=[{"role": "user", "content": "Hello from HyperInfer!"}],\n)\nprint(resp.choices[0].message.content)`;
            case 'anthropic':
                return `# Claude Code / Anthropic SDK drop-in\nexport ANTHROPIC_BASE_URL="${baseOrigin}"\nexport ANTHROPIC_API_KEY="${k}"\n\n# Or in Python:\nfrom anthropic import Anthropic\nclient = Anthropic(base_url="${baseOrigin}", api_key="${k}")\nmsg = client.messages.create(\n    model="${m}",\n    max_tokens=512,\n    messages=[{"role": "user", "content": "Hello!"}],\n)`;
            case 'gemini':
                return `# Google GenAI SDK (routes to /v1beta/models/${m}:generateContent)\nfrom google import genai\nfrom google.genai import types\n\nclient = genai.Client(\n    api_key="${k}",\n    http_options=types.HttpOptions(\n        base_url="${baseOrigin}",\n        api_version="v1beta",\n    ),\n)\nresp = client.models.generate_content(\n    model="${m}",\n    contents="Hello from HyperInfer!",\n)\nprint(resp.text)`;
            case 'responses':
                return `# OpenAI Responses API (routes to /v1/responses)\nfrom openai import OpenAI\n\nclient = OpenAI(\n    base_url="${baseOrigin}/v1",\n    api_key="${k}",\n)\nresp = client.responses.create(\n    model="${m}",\n    input="Hello from HyperInfer!",\n)\nprint(resp.output_text)`;
        }
    });

    function copySnippet() {
        navigator.clipboard?.writeText(snippetCode).then(() => {
            copiedSnippet = true;
            setTimeout(() => { copiedSnippet = false; }, 2000);
        }).catch(() => {});
    }

    async function sendChat() {
        responseError = '';
        responseText = '';
        lastLatencyMs = null;
        lastUsage = null;

        if (!apiKey.trim()) {
            responseError = 'Enter a Virtual API Key (hi-...) to send a live request.';
            return;
        }
        if (!userPrompt.trim()) {
            responseError = 'Enter a user message.';
            return;
        }

        sending = true;
        const start = performance.now();
        try {
            const messages: Array<{ role: string; content: string }> = [];
            if (systemPrompt.trim()) {
                messages.push({ role: 'system', content: systemPrompt.trim() });
            }
            messages.push({ role: 'user', content: userPrompt.trim() });

            const tempNum = Number(temperature);
            const maxNum = Number(maxTokens);
            const payload: Record<string, unknown> = {
                model: model.trim() || 'gpt-4o-mini',
                messages,
            };
            if (Number.isFinite(tempNum)) payload.temperature = tempNum;
            if (Number.isFinite(maxNum) && maxNum > 0) payload.max_tokens = Math.round(maxNum);

            const res = await fetch('/v1/chat/completions', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    Authorization: `Bearer ${apiKey.trim()}`,
                },
                body: JSON.stringify(payload),
            });

            lastLatencyMs = Math.round(performance.now() - start);
            const text = await res.text();
            if (!res.ok) {
                responseError = `Gateway returned ${res.status}: ${text}`;
                return;
            }
            const data = JSON.parse(text);
            responseText =
                data?.choices?.[0]?.message?.content ?? JSON.stringify(data, null, 2);
            if (data?.usage) {
                lastUsage = data.usage;
            }
        } catch (e) {
            lastLatencyMs = Math.round(performance.now() - start);
            responseError = e instanceof Error ? e.message : 'Request failed';
        } finally {
            sending = false;
        }
    }
</script>

<div class="space-y-8">
    <div>
        <h1 class="text-2xl font-bold text-[var(--on-surface)]">Playground</h1>
        <p class="text-sm text-[var(--on-surface-variant)] mt-1">
            Test configured models through the HyperInfer gateway and copy drop-in snippets for supported SDKs.
        </p>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <!-- Interactive Chat Panel -->
        <Card>
            <form class="space-y-4" onsubmit={(e) => { e.preventDefault(); sendChat(); }}>
                <div class="flex items-center justify-between">
                    <h2 class="text-base font-semibold text-[var(--on-surface)]">Test Chat Completion</h2>
                    {#if lastLatencyMs !== null}
                        <Badge variant="info" label={`${lastLatencyMs}ms`} />
                    {/if}
                </div>

                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <Input
                        label="Model or Alias"
                        list="playground-catalog-models"
                        bind:value={model}
                        placeholder={deployedModels.length > 0 || aliases.length > 0 ? 'Select active model or alias' : 'No active deployments yet'}
                    />
                    <datalist id="playground-catalog-models">
                        {#each aliases as alias}
                            <option value={alias.alias}>Alias → {alias.target_model}</option>
                        {/each}
                        {#each deployedModels as m}
                            <option value={m.id}>{m.owned_by}</option>
                        {/each}
                    </datalist>
                    <Input label="Virtual API Key" type="password" bind:value={apiKey} placeholder="hi-..." />
                </div>

                {#if aliases.length > 0 || deployedModels.length > 0}
                    <div class="space-y-1.5">
                        {#if aliases.length > 0}
                            <div class="flex flex-wrap items-center gap-1.5">
                                <span class="text-xs text-[var(--on-surface-variant)]">Team aliases:</span>
                                {#each aliases as alias}
                                    <button
                                        type="button"
                                        onclick={() => model = alias.alias}
                                        class="px-2 py-0.5 text-xs font-mono rounded border cursor-pointer transition-colors {model === alias.alias ? 'bg-[var(--primary)]/10 border-[var(--primary)] text-[var(--primary)]' : 'bg-[var(--surface-container-low)] border-[var(--ghost-border)] text-[var(--on-surface)] hover:bg-[var(--surface-container)]'}"
                                    >
                                        {alias.alias}
                                    </button>
                                {/each}
                            </div>
                        {/if}
                        {#if deployedModels.length > 0}
                            <div class="flex flex-wrap items-center gap-1.5">
                                <span class="text-xs text-[var(--on-surface-variant)]">Active deployments:</span>
                                {#each deployedModels as m}
                                    <button
                                        type="button"
                                        onclick={() => model = m.id}
                                        class="px-2 py-0.5 text-xs font-mono rounded border cursor-pointer transition-colors {model === m.id ? 'bg-[var(--primary)]/10 border-[var(--primary)] text-[var(--primary)]' : 'bg-[var(--surface-container-low)] border-[var(--ghost-border)] text-[var(--on-surface)] hover:bg-[var(--surface-container)]'}"
                                    >
                                        {m.id}
                                    </button>
                                {/each}
                            </div>
                        {/if}
                    </div>
                {:else}
                    <p class="text-xs text-[var(--on-surface-variant)]">
                        No active deployments or team aliases configured yet.{#if isAdmin} <a href="/dashboard/routing" class="text-[var(--primary)] font-medium hover:underline">Add a deployment in Routing →</a>{/if}
                    </p>
                {/if}

                <div class="grid grid-cols-2 gap-4">
                    <Input label="Temperature" type="text" bind:value={temperature} placeholder="0.7" />
                    <Input label="Max Tokens" type="number" bind:value={maxTokens} />
                </div>

                <div>
                    <label for="pg-system" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                        System Prompt
                    </label>
                    <textarea
                        id="pg-system"
                        bind:value={systemPrompt}
                        rows="2"
                        class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                    ></textarea>
                </div>

                <div>
                    <label for="pg-user" class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] mb-1">
                        User Message
                    </label>
                    <textarea
                        id="pg-user"
                        bind:value={userPrompt}
                        rows="3"
                        placeholder="Ask anything to test routing..."
                        class="w-full px-3 py-2 bg-[var(--surface-container-low)] text-[var(--on-surface)] placeholder:text-[var(--outline)] rounded-lg border-b-2 border-b-[var(--outline-variant)] focus:border-b-[var(--primary)] outline-none text-sm"
                    ></textarea>
                </div>

                <div class="flex justify-end">
                    <Button type="submit" disabled={sending}>
                        <span class="material-symbols-outlined" style="font-size: 18px">send</span>
                        {sending ? 'Sending...' : 'Send Request'}
                    </Button>
                </div>

                {#if responseError}
                    <div class="p-3 rounded-lg bg-[var(--error)]/10 text-[var(--error)] text-xs font-mono break-all">
                        {responseError}
                    </div>
                {/if}

                {#if responseText}
                    <div class="p-4 rounded-lg bg-[var(--surface-container-low)] space-y-2">
                        <div class="flex items-center justify-between">
                            <span class="text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]">Response</span>
                            {#if lastUsage}
                                <span class="text-xs font-mono text-[var(--on-surface-variant)]">
                                    in: {lastUsage.prompt_tokens ?? 0} · out: {lastUsage.completion_tokens ?? 0}
                                </span>
                            {/if}
                        </div>
                        <pre class="text-sm text-[var(--on-surface)] whitespace-pre-wrap font-sans">{responseText}</pre>
                    </div>
                {/if}
            </form>
        </Card>

        <!-- Multi-SDK Quickstart Snippets -->
        <Card>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <div>
                        <h2 class="text-base font-semibold text-[var(--on-surface)]">Connect SDK (Drop-In)</h2>
                        <p class="text-xs text-[var(--on-surface-variant)]">
                            Point any supported SDK at this HyperInfer gateway with a 1-line base URL change.
                        </p>
                    </div>
                    <Button variant="ghost" size="sm" onclick={copySnippet}>
                        <span class="material-symbols-outlined" style="font-size: 16px">{copiedSnippet ? 'check' : 'content_copy'}</span>
                        {copiedSnippet ? 'Copied!' : 'Copy'}
                    </Button>
                </div>

                <div class="flex flex-wrap gap-1 bg-[var(--surface-container-low)] rounded-lg p-1">
                    {#each [
                        { id: 'hyperinfer', label: 'HyperInfer SDK' },
                        { id: 'openai', label: 'OpenAI SDK' },
                        { id: 'anthropic', label: 'Anthropic / Claude Code' },
                        { id: 'gemini', label: 'Gemini SDK' },
                        { id: 'responses', label: 'Responses API' },
                    ] as tab}
                        <button
                            type="button"
                            class="px-3 py-1.5 text-xs font-medium rounded-md transition-all cursor-pointer {activeSdk === tab.id ? 'bg-[var(--surface-container-lowest)] text-[var(--primary)] shadow-sm' : 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'}"
                            onclick={() => activeSdk = tab.id as SdkTab}
                        >
                            {tab.label}
                        </button>
                    {/each}
                </div>

                <pre class="p-4 rounded-lg bg-[var(--surface-container-low)] text-xs font-mono text-[var(--on-surface)] overflow-x-auto leading-relaxed">{snippetCode}</pre>

                <div class="pt-2 border-t border-[var(--ghost-border)] grid grid-cols-2 gap-2 text-xs text-[var(--on-surface-variant)]">
                    <div><span class="font-semibold text-[var(--on-surface)]">Chat:</span> <code>/v1/chat/completions</code></div>
                    <div><span class="font-semibold text-[var(--on-surface)]">Responses:</span> <code>/v1/responses</code></div>
                    <div><span class="font-semibold text-[var(--on-surface)]">Anthropic:</span> <code>/v1/messages</code></div>
                    <div><span class="font-semibold text-[var(--on-surface)]">Gemini:</span> <code>/v1beta/models/:generateContent</code></div>
                </div>
            </div>
        </Card>
    </div>
</div>
