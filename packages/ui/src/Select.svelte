<script lang="ts" generics="T extends string = string">
	export interface SelectOption<V extends string = string> {
		value: V;
		label: string;
		disabled?: boolean;
	}

	interface Props {
		options: SelectOption<T>[];
		value?: T;
		label?: string;
		placeholder?: string;
		id?: string;
		ariaLabel?: string;
		disabled?: boolean;
		class?: string;
		onchange?: (value: T) => void;
	}

	const generatedId = $props.id();

	let {
		options,
		value = $bindable('' as T),
		label,
		placeholder = 'Select...',
		id = generatedId,
		ariaLabel,
		disabled = false,
		class: className = '',
		onchange
	}: Props = $props();

	let open = $state(false);
	let highlightedIndex = $state(-1);
	let triggerEl = $state<HTMLButtonElement | null>(null);
	let menuEl = $state<HTMLDivElement | null>(null);
	let menuStyle = $state('');

	let selectedOption = $derived(options.find((o) => o.value === value) ?? null);

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	function updatePosition() {
		if (!triggerEl || typeof window === 'undefined') return;
		const rect = triggerEl.getBoundingClientRect();
		const spaceBelow = window.innerHeight - rect.bottom - 12;
		const spaceAbove = rect.top - 12;
		const flipUp = spaceBelow < 180 && spaceAbove > spaceBelow;
		const maxHeight = Math.max(120, Math.min(260, flipUp ? spaceAbove : spaceBelow));
		const width = Math.min(Math.max(rect.width, 140), window.innerWidth - 16);
		const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));

		if (flipUp) {
			const bottom = window.innerHeight - rect.top + 4;
			menuStyle = `position:fixed;bottom:${Math.round(bottom)}px;left:${Math.round(left)}px;width:${Math.round(width)}px;max-height:${Math.round(maxHeight)}px;z-index:9999;`;
		} else {
			const top = rect.bottom + 4;
			menuStyle = `position:fixed;top:${Math.round(top)}px;left:${Math.round(left)}px;width:${Math.round(width)}px;max-height:${Math.round(maxHeight)}px;z-index:9999;`;
		}
	}

	function openMenu() {
		if (disabled || options.length === 0) return;
		const idx = options.findIndex((o) => o.value === value && !o.disabled);
		highlightedIndex = idx >= 0 ? idx : options.findIndex((o) => !o.disabled);
		updatePosition();
		open = true;
	}

	function closeMenu(restoreFocus = false) {
		open = false;
		if (restoreFocus) {
			triggerEl?.focus();
		}
	}

	function selectOption(opt: SelectOption<T>) {
		if (opt.disabled) return;
		const changed = value !== opt.value;
		value = opt.value;
		closeMenu(true);
		if (changed) {
			onchange?.(opt.value);
		}
	}

	function moveHighlight(delta: number) {
		if (options.length === 0) return;
		let idx = highlightedIndex;
		for (let step = 0; step < options.length; step++) {
			idx = (idx + delta + options.length) % options.length;
			if (!options[idx].disabled) {
				highlightedIndex = idx;
				const itemEl = menuEl?.querySelector<HTMLElement>(`[data-option-idx="${idx}"]`);
				itemEl?.scrollIntoView({ block: 'nearest' });
				break;
			}
		}
	}

	function handleTriggerKeydown(e: KeyboardEvent) {
		if (disabled) return;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			if (!open) openMenu();
			else moveHighlight(1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			if (!open) openMenu();
			else moveHighlight(-1);
		} else if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			if (!open) {
				openMenu();
			} else if (highlightedIndex >= 0 && options[highlightedIndex]) {
				selectOption(options[highlightedIndex]);
			}
		} else if (e.key === 'Tab' && open) {
			closeMenu(false);
		}
	}

	$effect(() => {
		if (!open || typeof window === 'undefined') return;
		updatePosition();

		const onReposition = () => updatePosition();
		const onPointerDown = (e: PointerEvent) => {
			const target = e.target as Node | null;
			if (target && (triggerEl?.contains(target) || menuEl?.contains(target))) {
				return;
			}
			closeMenu(false);
		};
		const onWindowKeydown = (e: KeyboardEvent) => {
			if (e.key === 'Escape') {
				e.stopPropagation();
				e.preventDefault();
				closeMenu(true);
			}
		};

		window.addEventListener('scroll', onReposition, { capture: true, passive: true });
		window.addEventListener('resize', onReposition, { passive: true });
		window.addEventListener('pointerdown', onPointerDown, { capture: true });
		window.addEventListener('keydown', onWindowKeydown, { capture: true });

		return () => {
			window.removeEventListener('scroll', onReposition, { capture: true });
			window.removeEventListener('resize', onReposition);
			window.removeEventListener('pointerdown', onPointerDown, { capture: true });
			window.removeEventListener('keydown', onWindowKeydown, { capture: true });
		};
	});
</script>

<div class="space-y-1.5 {className}">
	{#if label}
		<label
			for={id}
			class="block text-xs font-medium uppercase tracking-wider text-[var(--on-surface-variant)]"
			style="font-family: 'Inter', sans-serif"
		>
			{label}
		</label>
	{/if}
	<button
		bind:this={triggerEl}
		{id}
		type="button"
		role="combobox"
		aria-label={ariaLabel}
		aria-expanded={open}
		aria-haspopup="listbox"
		aria-controls="{id}-listbox"
		{disabled}
		onclick={() => (open ? closeMenu(false) : openMenu())}
		onkeydown={handleTriggerKeydown}
		class="h-10 w-full px-3 flex items-center justify-between gap-2 text-left text-sm bg-[var(--surface-container-low)] text-[var(--on-surface)] rounded-lg border transition-colors duration-200 outline-none cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed {open ? 'bg-[var(--surface-container-lowest)] border-[var(--primary)]' : 'border-[var(--outline-variant)] hover:border-[var(--outline)] focus:bg-[var(--surface-container-lowest)] focus:border-[var(--primary)]'}"
	>
		<span class="truncate {selectedOption ? 'text-[var(--on-surface)]' : 'text-[var(--outline)]'}">
			{selectedOption ? selectedOption.label : placeholder}
		</span>
		<span
			aria-hidden="true"
			class="material-symbols-outlined text-[var(--on-surface-variant)] shrink-0 transition-transform duration-150 {open ? 'rotate-180' : ''}"
			style="font-size: 18px"
		>
			expand_more
		</span>
	</button>
</div>

{#if open}
	<div
		use:portal
		bind:this={menuEl}
		id="{id}-listbox"
		role="listbox"
		tabindex="-1"
		style={menuStyle}
		class="overflow-y-auto rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] shadow-[0_12px_32px_rgba(var(--shadow-color),0.18)] py-1"
	>
		{#each options as opt, idx}
			{@const selected = opt.value === value}
			{@const highlighted = idx === highlightedIndex}
			<button
				type="button"
				role="option"
				data-option-idx={idx}
				aria-selected={selected}
				disabled={opt.disabled}
				onmouseenter={() => { if (!opt.disabled) highlightedIndex = idx; }}
				onclick={() => selectOption(opt)}
				class="w-full px-3 py-2 text-left text-sm flex items-center justify-between gap-2 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed {selected
					? 'text-[var(--primary)] font-medium bg-[var(--primary)]/10'
					: highlighted
						? 'bg-[var(--surface-container)] text-[var(--on-surface)]'
						: 'text-[var(--on-surface)] hover:bg-[var(--surface-container)]'}"
			>
				<span class="truncate">{opt.label}</span>
				{#if selected}
					<span aria-hidden="true" class="material-symbols-outlined text-[var(--primary)] shrink-0" style="font-size: 16px">
						check
					</span>
				{/if}
			</button>
		{/each}
	</div>
{/if}
