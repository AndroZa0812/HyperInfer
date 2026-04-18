<script lang="ts">
    import VirtualList from 'svelte-virtual-list';

    let { items = [], children }: { items: any[], children: any } = $props();
</script>

<div class="h-full overflow-auto">
    <VirtualList {items} let:item>
        {@render children({ item })}
    </VirtualList>
</div>