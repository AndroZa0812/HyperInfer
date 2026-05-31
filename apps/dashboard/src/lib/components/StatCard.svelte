<script lang="ts">
	interface Props {
		icon?: string;
		label: string;
		value: string;
		trend?: string;
		trendDirection?: 'up' | 'down' | 'neutral';
		class?: string;
	}

	let {
		icon,
		label,
		value,
		trend,
		trendDirection = 'neutral',
		class: className = ''
	}: Props = $props();

	const trendColors = {
		up: 'text-[var(--success)]',
		down: 'text-[var(--error)]',
		neutral: 'text-[var(--on-surface-variant)]'
	};

	const trendIcons = {
		up: 'trending_up',
		down: 'trending_down',
		neutral: 'remove'
	};
</script>

<div class="bg-[var(--surface-container-lowest)] rounded-xl p-5 space-y-3 {className}">
	<div class="flex items-center justify-between">
		{#if icon}
			<div class="w-10 h-10 rounded-lg bg-[var(--surface-container)] flex items-center justify-center">
				<span class="material-symbols-outlined text-[var(--primary)]" style="font-size: 20px">{icon}</span>
			</div>
		{/if}
		{#if trend}
			<div class="flex items-center gap-1 text-xs font-medium {trendColors[trendDirection]}">
				<span class="material-symbols-outlined" style="font-size: 16px">{trendIcons[trendDirection]}</span>
				{trend}
			</div>
		{/if}
	</div>
	<div>
		<p class="text-2xl font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">{value}</p>
		<p class="text-xs uppercase tracking-wider text-[var(--on-surface-variant)] mt-1">{label}</p>
	</div>
</div>
