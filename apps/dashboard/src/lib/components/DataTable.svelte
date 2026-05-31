<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Column {
		key: string;
		label: string;
		class?: string;
	}

	interface Props {
		columns: Column[];
		rows: Record<string, any>[];
		emptyMessage?: string;
		children?: Snippet<[Record<string, any>]>;
	}

	let {
		columns,
		rows,
		emptyMessage = 'No data',
		children
	}: Props = $props();
</script>

<div class="bg-[var(--surface-container-lowest)] rounded-xl overflow-hidden">
	{#if rows.length === 0}
		<div class="p-8 text-center text-[var(--on-surface-variant)]">
			{emptyMessage}
		</div>
	{:else}
		<div class="overflow-x-auto">
			<table class="w-full">
				<thead>
					<tr class="bg-[var(--surface-container-low)]">
						{#each columns as col}
							<th class="px-5 py-3.5 text-left text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)] {col.class || ''}">
								{col.label}
							</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each rows as row, i}
						<tr class="{i > 0 ? 'border-t border-[var(--ghost-border)]' : ''} hover:bg-[var(--surface-container-low)]/50 transition-colors">
							{#if children}
								{@render children(row)}
							{:else}
								{#each columns as col}
									<td class="px-5 py-4 text-sm text-[var(--on-surface)] {col.class || ''}">
										{row[col.key] ?? ''}
									</td>
								{/each}
							{/if}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
