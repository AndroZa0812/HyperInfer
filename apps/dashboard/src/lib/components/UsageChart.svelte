<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as echarts from 'echarts';

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
	let chart: echarts.ECharts;

	const primaryColor = 'var(--primary)';
	const primaryContainer = 'var(--primary-container)';

	$effect(() => {
		if (chart && data.length) {
			const options: echarts.EChartsOption = {
				tooltip: {
					trigger: 'axis',
					backgroundColor: 'var(--surface-container-lowest)',
					borderColor: 'var(--ghost-border)',
					textStyle: { color: 'var(--on-surface)', fontFamily: 'Inter' }
				},
				xAxis: type !== 'donut' ? {
					type: 'category',
					data: data.map((d: DataPoint) => d.date),
					axisLine: { lineStyle: { color: 'var(--outline-variant)' } },
					axisLabel: { color: 'var(--on-surface-variant)', fontFamily: 'Inter', fontSize: 11 },
				} : undefined,
				yAxis: type !== 'donut' ? {
					type: 'value',
					axisLine: { show: false },
					splitLine: { lineStyle: { color: 'var(--ghost-border)' } },
					axisLabel: { color: 'var(--on-surface-variant)', fontFamily: 'Inter', fontSize: 11 },
				} : undefined,
				series: [{
					type: type === 'donut' ? 'pie' : type,
					data: type === 'donut'
						? data.map((d: DataPoint) => ({ name: d.date, value: d.cost }))
						: data.map((d: DataPoint) => type === 'line' ? d.tokens : d.cost),
					...(type === 'line' ? {
						smooth: true,
						lineStyle: { width: 2.5, color: primaryColor },
						areaStyle: {
							color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
								{ offset: 0, color: 'rgba(70, 72, 212, 0.15)' },
								{ offset: 1, color: 'rgba(70, 72, 212, 0.01)' }
							])
						},
						itemStyle: { color: primaryColor },
					} : {}),
					...(type === 'bar' ? {
						itemStyle: {
							color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
								{ offset: 0, color: primaryColor },
								{ offset: 1, color: primaryContainer }
							]),
							borderRadius: [4, 4, 0, 0]
						},
					} : {}),
					...(type === 'donut' ? {
						radius: ['50%', '75%'],
						itemStyle: { borderRadius: 6, borderColor: 'var(--surface-container-lowest)', borderWidth: 2 },
						label: { show: false },
					} : {}),
				}],
				grid: type !== 'donut' ? { left: 48, right: 16, top: 16, bottom: 32 } : undefined,
			};
			chart.setOption(options);
		}
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
