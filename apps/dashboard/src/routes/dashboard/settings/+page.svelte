<script lang="ts">
    import { auth } from '$lib/stores/auth';
    import { theme } from '$lib/stores/theme';

    let currentPassword = '';
    let newPassword = '';
    let confirmPassword = '';
    let saving = false;
    let success = '';
</script>

<div class="space-y-6 max-w-2xl">
    <h1 class="text-2xl font-bold">Settings</h1>

    <div class="bg-[var(--bg-primary)] rounded-xl p-6 space-y-6">
        <h2 class="text-lg font-medium">Profile</h2>
        <div>
            <label class="block text-sm font-medium mb-1">Email</label>
            <input
                type="email"
                value={$auth.user?.email || ''}
                disabled
                class="w-full px-4 py-2 border rounded-lg opacity-60"
            />
        </div>
        <div>
            <label class="block text-sm font-medium mb-1">Role</label>
            <input
                type="text"
                value={$auth.user?.role || ''}
                disabled
                class="w-full px-4 py-2 border rounded-lg opacity-60"
            />
        </div>
    </div>

    <div class="bg-[var(--bg-primary)] rounded-xl p-6 space-y-6">
        <h2 class="text-lg font-medium">Appearance</h2>
        <div class="flex items-center gap-4">
            <span>Theme:</span>
            <button
                on:click={() => theme.toggle()}
                class="px-4 py-2 border rounded-lg"
            >
                {$theme === 'light' ? 'Switch to Dark' : 'Switch to Light'}
            </button>
        </div>
    </div>

    <div class="bg-[var(--bg-primary)] rounded-xl p-6 space-y-6">
        <h2 class="text-lg font-medium">Change Password</h2>
        <div class="space-y-4">
            <div>
                <label class="block text-sm font-medium mb-1">Current Password</label>
                <input
                    type="password"
                    bind:value={currentPassword}
                    class="w-full px-4 py-2 border rounded-lg"
                />
            </div>
            <div>
                <label class="block text-sm font-medium mb-1">New Password</label>
                <input
                    type="password"
                    bind:value={newPassword}
                    class="w-full px-4 py-2 border rounded-lg"
                />
            </div>
            <div>
                <label class="block text-sm font-medium mb-1">Confirm Password</label>
                <input
                    type="password"
                    bind:value={confirmPassword}
                    class="w-full px-4 py-2 border rounded-lg"
                />
            </div>
            <button
                class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg"
                disabled={saving}
            >
                {saving ? 'Saving...' : 'Update Password'}
            </button>
            {#if success}
                <p class="text-green-600 text-sm">{success}</p>
            {/if}
        </div>
    </div>
</div>
