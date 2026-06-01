<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'default' | 'elevated' | 'outlined';
		padding?: 'none' | 'sm' | 'md' | 'lg';
		class?: string;
		children: Snippet;
	}

	let {
		variant = 'default',
		padding = 'md',
		class: className = '',
		children
	}: Props = $props();

	const variants = {
		default: 'bg-[var(--surface-container-lowest)]',
		elevated: 'bg-[var(--surface-container-lowest)] shadow-[0_24px_48px_rgba(var(--shadow-color),0.06)]',
		outlined: 'bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)]'
	};

	const paddings = {
		none: '',
		sm: 'p-4',
		md: 'p-6',
		lg: 'p-8'
	};

	const classes = $derived(`rounded-xl ${variants[variant]} ${paddings[padding]} ${className}`);
</script>

<div class={classes}>
	{@render children()}
</div>
