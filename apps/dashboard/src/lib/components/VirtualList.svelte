<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props<T> {
		items: T[];
		children: Snippet<[T]>;
		height?: string;
	}

	let { items, children, height = 'h-full' }: Props<unknown> = $props();

	let containerEl: HTMLDivElement;
	let scrollTop = $state(0);
	let containerHeight = $state(0);

	const itemHeight = 80;
	const buffer = 3;

	let startIndex = $derived(Math.max(0, Math.floor(scrollTop / itemHeight) - buffer));
	let endIndex = $derived(Math.min(items.length, Math.ceil((scrollTop + containerHeight) / itemHeight) + buffer));

	function getVisibleItems<T>(items: T[], start: number, end: number): T[] {
		return items.slice(start, end);
	}

	let visibleItems = $derived(getVisibleItems(items, startIndex, endIndex));
	let totalHeight = $derived(items.length * itemHeight);
	let offsetY = $derived(startIndex * itemHeight);

	function onScroll(e: Event) {
		const target = e.target as HTMLDivElement;
		scrollTop = target.scrollTop;
	}
</script>

<div
	bind:this={containerEl}
	class="overflow-auto {height}"
	onscroll={onScroll}
	bind:clientHeight={containerHeight}
>
	<div class="relative" style="height: {totalHeight}px">
		<div style="transform: translateY({offsetY}px)">
			{#each visibleItems as item, i (startIndex + i)}
				{@render children(item)}
			{/each}
		</div>
	</div>
</div>