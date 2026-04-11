<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { auth } from '$lib/stores/auth';
    import Sidebar from '$lib/components/Sidebar.svelte';

    onMount(() => {
        auth.init();
    });

    $: if (!$auth.loading && !$auth.user) {
        goto('/login');
    }
</script>

{#if $auth.user}
    <Sidebar>
        <slot />
    </Sidebar>
{/if}
