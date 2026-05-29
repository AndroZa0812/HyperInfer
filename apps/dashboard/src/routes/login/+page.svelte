<script lang="ts">
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';
    import { theme } from '$lib/stores/theme';

    let email = $state('');
    let password = $state('');
    let error = $state('');
    let loading = $state(false);

    async function handleSubmit(e: Event) {
        e.preventDefault();
        loading = true;
        error = '';

        try {
            await auth.login(email, password);
            goto('/dashboard');
        } catch (e) {
            error = 'Invalid credentials';
        } finally {
            loading = false;
        }
    }
</script>

<div class="min-h-screen flex items-center justify-center bg-[var(--bg-secondary)]">
    <div class="bg-[var(--bg-primary)] rounded-xl p-8 w-full max-w-md shadow-lg">
        <div class="flex justify-between items-center mb-6">
            <h1 class="text-2xl font-bold text-[var(--text-primary)]">HyperInfer</h1>
            <button
                class="p-2 rounded-lg hover:bg-[var(--bg-secondary)]"
                onclick={() => theme.toggle()}
            >
                {#if $theme === 'light'}
                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 12c0 5.385 4.365 9.75 9.75 9.75.721 0 1.42-.078 2.092-.227A9.72 9.72 0 0 0 21.752 15.002z" />
                    </svg>
                {:else}
                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
                    </svg>
                {/if}
            </button>
        </div>

        <form onsubmit={handleSubmit} class="space-y-4">
            <div>
                <label class="block text-sm font-medium mb-1" for="email">Email</label>
                <input
                    id="email"
                    type="email"
                    bind:value={email}
                    class="w-full px-4 py-2 rounded-lg border border-[var(--bg-secondary)] bg-[var(--bg-primary)]"
                    required
                />
            </div>

            <div>
                <label class="block text-sm font-medium mb-1" for="password">Password</label>
                <input
                    id="password"
                    type="password"
                    bind:value={password}
                    class="w-full px-4 py-2 rounded-lg border border-[var(--bg-secondary)] bg-[var(--bg-primary)]"
                    required
                />
            </div>

            {#if error}
                <p class="text-red-500 text-sm">{error}</p>
            {/if}

            <button
                type="submit"
                class="w-full py-2 rounded-lg bg-[var(--accent)] text-white font-medium hover:opacity-90 disabled:opacity-50"
                disabled={loading}
            >
                {loading ? 'Signing in...' : 'Sign In'}
            </button>
        </form>
    </div>
</div>