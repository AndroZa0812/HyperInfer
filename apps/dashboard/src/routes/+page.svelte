<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';

    onMount(async () => {
        await auth.init();
        goto('/dashboard');
    });
</script>
