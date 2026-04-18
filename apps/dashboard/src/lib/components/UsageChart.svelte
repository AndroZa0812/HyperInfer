<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import * as echarts from 'echarts';

    interface Props {
        data: { date: string; tokens: number; cost: number; latency_ms: number }[];
        type: 'line' | 'bar' | 'donut' = 'line';
    }

    let { data = [], type = 'line' }: Props = $props();

    let chartEl: HTMLElement;
    let chart: echarts.ECharts;

    $effect(() => {
        if (chart && data.length) {
            const options: echarts.EChartsOption = {
                tooltip: { trigger: 'axis' },
                xAxis: {
                    type: 'category',
                    data: data.map(d => d.date),
                },
                yAxis: { type: 'value' },
                series: [{
                    type: type === 'donut' ? 'pie' : type,
                    data: type === 'donut'
                        ? data.map(d => ({ name: d.date, value: d.cost }))
                        : data.map(d => type === 'line' ? d.tokens : d.cost),
                    ...(type === 'donut' ? { radius: ['40%', '70%'] } : {}),
                }],
                grid: { left: 40, right: 40, top: 20, bottom: 20 },
            };
            chart.setOption(options);
        }
    });

    onMount(() => {
        chart = echarts.init(chartEl);
    });

    onDestroy(() => {
        chart?.dispose();
    });
</script>

<div bind:this={chartEl} class="w-full h-64"></div>