<script>
    import VirtualList from 'svelte-virtual-list';

    export let items = [];
</script>

<div class="h-full overflow-auto">
    <VirtualList {items} let:item>
        <slot {item} />
    </VirtualList>
</div>
