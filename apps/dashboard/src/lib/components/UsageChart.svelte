<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as echarts from 'echarts';
	import { theme } from '$lib/stores/theme';

	interface DataPoint {
		date: string;
		tokens: number;
		cost: number;
		latency_ms: number;
	}

	interface Props {
		data?: DataPoint[];
		type?: 'line' | 'bar' | 'donut';
		class?: string;
	}

	let { data = [], type = 'line', class: className = '' }: Props = $props();

	let chartEl: HTMLElement;
	let chart = $state<echarts.ECharts | null>(null);

	function resolveColor(varName: string, fallback: string): string {
		try {
			const computed = getComputedStyle(document.documentElement);
			const value = computed.getPropertyValue(varName).trim();
			if (value) return value;
		} catch {
			// ignore
		}
		return fallback;
	}

	$effect(() => {
		const _currentTheme = $theme;
		void _currentTheme;
		if (!chart || !data.length) return;
		const primary = resolveColor('--primary', '#4648d4');
		const primaryContainer = resolveColor('--primary-container', '#6063ee');
		const surfaceLowest = resolveColor('--surface-container-lowest', '#ffffff');
		const ghostBorder = resolveColor('--ghost-border', '#131b2e');
		const outlineVariant = resolveColor('--outline-variant', '#464554');
		const onSurface = resolveColor('--on-surface', '#131b2e');
		const onSurfaceVariant = resolveColor('--on-surface-variant', '#464554');

		const options: echarts.EChartsOption = {
			tooltip: {
				trigger: 'axis',
				backgroundColor: surfaceLowest,
				borderColor: ghostBorder,
				textStyle: { color: onSurface, fontFamily: 'Inter' }
			},
			xAxis: type !== 'donut' ? {
				type: 'category',
				data: data.map((d: DataPoint) => d.date),
				axisLine: { lineStyle: { color: outlineVariant } },
				axisLabel: { color: onSurfaceVariant, fontFamily: 'Inter', fontSize: 11 },
			} : undefined,
			yAxis: type !== 'donut' ? {
				type: 'value',
				axisLine: { show: false },
				splitLine: { lineStyle: { color: ghostBorder } },
				axisLabel: { color: onSurfaceVariant, fontFamily: 'Inter', fontSize: 11 },
			} : undefined,
			series: [{
				type: type === 'donut' ? 'pie' : type,
				data: type === 'donut'
					? data.map((d: DataPoint) => ({ name: d.date, value: d.cost }))
					: data.map((d: DataPoint) => type === 'line' ? d.tokens : d.cost),
				...(type === 'line' ? {
					smooth: true,
					lineStyle: { width: 2.5, color: primary },
					areaStyle: {
						color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
							{ offset: 0, color: 'rgba(70, 72, 212, 0.15)' },
							{ offset: 1, color: 'rgba(70, 72, 212, 0.01)' }
						])
					},
					itemStyle: { color: primary },
				} : {}),
				...(type === 'bar' ? {
					itemStyle: {
						color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
							{ offset: 0, color: primary },
							{ offset: 1, color: primaryContainer }
						]),
						borderRadius: [4, 4, 0, 0]
					},
				} : {}),
				...(type === 'donut' ? {
					radius: ['50%', '75%'],
					itemStyle: { borderRadius: 6, borderColor: surfaceLowest, borderWidth: 2 },
					label: { show: false },
				} : {}),
			}],
			grid: type !== 'donut' ? { left: 48, right: 16, top: 16, bottom: 32 } : undefined,
		};
		chart.setOption(options);
	});

	onMount(() => {
		chart = echarts.init(chartEl);
		const observer = new ResizeObserver(() => chart?.resize());
		observer.observe(chartEl);
		return () => observer.disconnect();
	});

	onDestroy(() => {
		chart?.dispose();
	});
</script>

<div bind:this={chartEl} class="w-full h-64 {className}"></div>