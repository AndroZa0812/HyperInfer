<script lang="ts">
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';
    import { theme } from '$lib/stores/theme';
    import Button from '$lib/components/Button.svelte';
    import Input from '$lib/components/Input.svelte';

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

<div class="min-h-screen flex">
    <div class="hidden lg:flex lg:w-[60%] relative overflow-hidden items-center justify-center" style="background: linear-gradient(135deg, var(--primary) 0%, var(--primary-container) 50%, var(--secondary) 100%);">
        <div class="absolute inset-0 opacity-20">
            <div class="absolute top-1/4 left-1/4 w-96 h-96 rounded-full" style="background: radial-gradient(circle, var(--primary-fixed) 0%, transparent 70%); filter: blur(60px);"></div>
            <div class="absolute bottom-1/4 right-1/4 w-80 h-80 rounded-full" style="background: radial-gradient(circle, var(--secondary-container) 0%, transparent 70%); filter: blur(80px);"></div>
        </div>

        <div class="relative z-10 max-w-lg px-12 text-center">
            <div class="w-16 h-16 rounded-2xl flex items-center justify-center mx-auto mb-8" style="background: rgba(255,255,255,0.15); backdrop-filter: blur(10px);">
                <span class="material-symbols-outlined text-white" style="font-size: 36px">bolt</span>
            </div>
            <h1 class="text-5xl font-extrabold text-white mb-4" style="font-family: 'Manrope', sans-serif">HyperInfer</h1>
            <p class="text-lg text-white/70 font-light tracking-wide">Next-Generation LLM Gateway</p>
            <div class="mt-12 flex items-center justify-center gap-8 text-white/50 text-xs uppercase tracking-[0.2em]">
                <span>Enterprise</span>
                <span class="w-1 h-1 rounded-full bg-white/30"></span>
                <span>Scalable</span>
                <span class="w-1 h-1 rounded-full bg-white/30"></span>
                <span>Secure</span>
            </div>
        </div>
    </div>

    <div class="flex-1 flex items-center justify-center bg-[var(--surface)] p-8">
        <div class="w-full max-w-md">
            <div class="flex justify-between items-center mb-12">
                <div class="lg:hidden flex items-center gap-2">
                    <div class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: linear-gradient(135deg, var(--primary), var(--primary-container))">
                        <span class="material-symbols-outlined text-[var(--on-primary)]" style="font-size: 18px">bolt</span>
                    </div>
                    <span class="text-lg font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">HyperInfer</span>
                </div>
                <button
                    class="p-2 rounded-lg hover:bg-[var(--surface-container)] transition-colors cursor-pointer ml-auto"
                    onclick={() => theme.toggle()}
                >
                    <span class="material-symbols-outlined text-[var(--on-surface-variant)]" style="font-size: 20px">
                        {$theme === 'light' ? 'dark_mode' : 'light_mode'}
                    </span>
                </button>
            </div>

            <div class="mb-8">
                <p class="text-xs uppercase tracking-[0.2em] text-[var(--on-surface-variant)] mb-2">Authentication Gateway</p>
                <h2 class="text-3xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">Welcome Back</h2>
            </div>

            <form onsubmit={handleSubmit} class="space-y-6">
                <Input
                    id="email"
                    label="Email Address"
                    type="email"
                    bind:value={email}
                    required
                />

                <div class="relative">
                    <Input
                        id="password"
                        label="Password"
                        type="password"
                        bind:value={password}
                        required
                    />
                    <button type="button" class="absolute right-0 top-0 text-xs text-[var(--primary)] hover:underline cursor-pointer">
                        Forgot password?
                    </button>
                </div>

                {#if error}
                    <div class="flex items-center gap-2 text-sm text-[var(--error)] bg-[var(--error-container)]/20 rounded-lg px-4 py-3">
                        <span class="material-symbols-outlined" style="font-size: 18px">error</span>
                        {error}
                    </div>
                {/if}

                <Button type="submit" size="lg" disabled={loading} class="w-full">
                    {#if loading}
                        <span class="material-symbols-outlined animate-spin" style="font-size: 18px">progress_activity</span>
                        Signing in...
                    {:else}
                        Sign In
                    {/if}
                </Button>
            </form>

            <div class="mt-8">
                <div class="flex items-center gap-4 mb-6">
                    <div class="flex-1 h-px bg-[var(--ghost-border)]"></div>
                    <span class="text-xs text-[var(--on-surface-variant)] uppercase tracking-wider">or</span>
                    <div class="flex-1 h-px bg-[var(--ghost-border)]"></div>
                </div>

                <div class="grid grid-cols-2 gap-3">
                    <button class="flex items-center justify-center gap-2 px-4 py-3 rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] hover:border-[var(--ghost-border-hover)] transition-all text-sm text-[var(--on-surface-variant)] cursor-pointer">
                        <svg class="w-4 h-4" viewBox="0 0 24 24"><path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 01-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"/><path fill="currentColor" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/><path fill="currentColor" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/><path fill="currentColor" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/></svg>
                        Google
                    </button>
                    <button class="flex items-center justify-center gap-2 px-4 py-3 rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] hover:border-[var(--ghost-border-hover)] transition-all text-sm text-[var(--on-surface-variant)] cursor-pointer">
                        <span class="material-symbols-outlined" style="font-size: 18px">terminal</span>
                        SSO
                    </button>
                </div>
            </div>

            <p class="mt-8 text-center text-sm text-[var(--on-surface-variant)]">
                New to HyperInfer? <button class="text-[var(--primary)] font-medium hover:underline cursor-pointer">Request Access</button>
            </p>

            <div class="mt-12 flex items-center justify-center gap-6 text-xs text-[var(--outline)]">
                <span>&copy; 2024 HyperInfer. Precision Engineered.</span>
            </div>
        </div>
    </div>
</div>
