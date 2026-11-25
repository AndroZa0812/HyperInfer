<script lang="ts">
    import { goto } from '$app/navigation';
    import { auth } from '#lib/stores/auth';
    import { theme } from '#lib/stores/theme';
    import Button from '#lib/components/Button.svelte';
    import Input from '#lib/components/Input.svelte';

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

                <Input
                    id="password"
                    label="Password"
                    type="password"
                    bind:value={password}
                    required
                />

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



            <div class="mt-12 flex items-center justify-center gap-6 text-xs text-[var(--outline)]">
                <span>&copy; 2027 HyperInfer. Precision Engineered.</span>
            </div>
        </div>
    </div>
</div>
