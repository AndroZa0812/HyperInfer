<script lang="ts">
    import '../app.css';
    import { onMount } from 'svelte';
    import { theme } from '$lib/stores/theme';
    import { auth } from '$lib/stores/auth';
    import type { Snippet } from 'svelte';

    interface Props {
        children: Snippet;
    }

    let { children } = $props<Props>();

    onMount(() => {
        theme.init();
        auth.init();
    });
</script>

{@render children()}