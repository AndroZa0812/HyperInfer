<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'primary' | 'secondary' | 'ghost';
		size?: 'sm' | 'md' | 'lg';
		disabled?: boolean;
		href?: string;
		onclick?: (e: MouseEvent) => void;
		type?: 'button' | 'submit' | 'reset';
		children: Snippet;
		class?: string;
	}

	let {
		variant = 'primary',
		size = 'md',
		disabled = false,
		href,
		onclick,
		type = 'button',
		children,
		class: className = ''
	}: Props = $props();

	const base = 'inline-flex items-center justify-center font-medium rounded-lg transition-all duration-200 cursor-pointer select-none';

	const variants = {
		primary: 'text-[var(--on-primary)] shadow-sm hover:opacity-90 active:scale-[0.98]',
		secondary: 'bg-transparent text-[var(--primary)] hover:bg-[var(--surface-container)] active:scale-[0.98]',
		ghost: 'bg-transparent text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]'
	};

	const sizes = {
		sm: 'px-3 py-1.5 text-sm gap-1.5',
		md: 'px-5 py-2.5 text-sm gap-2',
		lg: 'px-6 py-3 text-base gap-2.5'
	};

	const classes = $derived(`${base} ${variants[variant]} ${sizes[size]} ${disabled ? 'opacity-50 pointer-events-none' : ''} ${className}`);
</script>

{#if href && !disabled}
	<a {href} class={classes} onclick={onclick}>
		{@render children()}
	</a>
{:else}
	<button {type} {disabled} class={classes} onclick={onclick} style={variant === 'primary' ? 'background: linear-gradient(135deg, var(--primary), var(--primary-container))' : ''}>
		{@render children()}
	</button>
{/if}
