<script lang="ts">
    import VirtualList from 'svelte-virtual-list';
    import type { Snippet } from 'svelte';

    interface Props {
        items: any[];
        children: Snippet<[{ item: any }];
    }

    let { items = [], children }: Props = $props();
</script>

<div class="h-full overflow-auto">
    <VirtualList {items} let:item>
        {@render children({ item })}
    </VirtualList>
</div>