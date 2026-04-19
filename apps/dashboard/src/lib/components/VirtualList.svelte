<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import type { Action } from 'svelte/action';

	interface Props<T> {
		items: T[];
		children: Snippet<[T]>;
		height?: string;
		estimatedItemHeight?: number;
	}

	let { items, children, height = 'h-full', estimatedItemHeight = 80 }: Props<T> = $props();

	let containerEl: HTMLDivElement;
	let scrollTop = $state(0);

	let heights = $state<Map<number, number>>(new Map());
	let itemRefs: HTMLDivElement[] = [];

	const buffer = 3;

	function getHeight(index: number): number {
		return heights.get(index) ?? estimatedItemHeight;
	}

	function computeCumulativeOffsets(): number[] {
		const offsets: number[] = [];
		let offset = 0;
		for (let i = 0; i < items.length; i++) {
			offsets.push(offset);
			offset += getHeight(i);
		}
		return offsets;
	}

	const measureItem: Action<HTMLDivElement, (el: HTMLDivElement) => void> = (el, callback) => {
		callback(el);
		const ro = new ResizeObserver((entries) => {
			for (const entry of entries) {
				const newHeight = entry.borderBoxSize[0]?.blockSize ?? el.offsetHeight;
				const idx = itemRefs.indexOf(el);
				if (idx !== -1 && heights.get(idx) !== newHeight) {
					const newHeights = new Map(heights);
					newHeights.set(idx, newHeight);
					heights = newHeights;
				}
			}
		});
		ro.observe(el);
		return {
			destroy() {
				ro.disconnect();
			}
		};
	};

	$effect(() => {
		itemRefs = new Array(items.length).fill(null);
		heights = new Map();
	});

	let cumulativeOffsets = $derived(computeCumulativeOffsets());
	let totalHeight = $derived(cumulativeOffsets[items.length] ?? (items.length * estimatedItemHeight));

	function binarySearchStart(scrollTop: number): number {
		if (items.length === 0) return 0;
		const total = cumulativeOffsets.length;
		let lo = 0;
		let hi = total - 1;
		while (lo <= hi) {
			const mid = (lo + hi) >>> 1;
			const off = cumulativeOffsets[mid];
			if (off < scrollTop - buffer * estimatedItemHeight) {
				lo = mid + 1;
			} else {
				hi = mid - 1;
			}
		}
		return Math.max(0, lo - buffer);
	}

	function binarySearchEnd(scrollTop: number, viewHeight: number): number {
		if (items.length === 0) return 0;
		const total = cumulativeOffsets.length;
		let lo = 0;
		let hi = total - 1;
		while (lo <= hi) {
			const mid = (lo + hi) >>> 1;
			const off = cumulativeOffsets[mid];
			if (off < scrollTop + viewHeight + buffer * estimatedItemHeight) {
				lo = mid + 1;
			} else {
				hi = mid - 1;
			}
		}
		return Math.min(total, lo + buffer);
	}

	function getStartEndIndices(): [number, number] {
		const vh = containerEl?.clientHeight ?? 0;
		const maxScroll = Math.max(0, totalHeight - vh);
		const st = Math.min(containerEl?.scrollTop ?? 0, maxScroll);
		const start = binarySearchStart(st);
		const end = binarySearchEnd(st, vh);
		return [Math.min(start, items.length), Math.min(end, items.length)];
	}

	let [startIndex, endIndex] = $derived(getStartEndIndices());
	let visibleItems = $derived(items.slice(startIndex, endIndex));
	let offsetY = $derived(cumulativeOffsets[startIndex] ?? 0);

	function onScroll() {
		scrollTop = containerEl.scrollTop;
	}
</script>

<div
	bind:this={containerEl}
	class="overflow-auto {height}"
	onscroll={onScroll}
>
	<div class="relative" style="height: {totalHeight}px">
		<div style="transform: translateY({offsetY}px)">
			{#each visibleItems as item, i (startIndex + i)}
				{@const idx = startIndex + i}
				<div use:measureItem={(el) => { itemRefs[idx] = el; }} style="height: {getHeight(idx)}px">
					{@render children(item)}
				</div>
			{/each}
		</div>
	</div>
</div>