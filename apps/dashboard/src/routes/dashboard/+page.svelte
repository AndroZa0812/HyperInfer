<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';

    onMount(() => {
        if ($auth.user?.role === 'admin') {
            goto('/dashboard/teams');
        } else {
            goto('/dashboard/keys');
        }
    });
</script>

<p>Redirecting...</p>
