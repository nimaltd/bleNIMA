<script lang="ts" module>
  /** Colors for the lines, the same in both themes, the app's own two first. */
  export const SERIES_COLORS = ["#2dd4bf", "#fb923c", "#60a5fa", "#c084fc", "#facc15", "#4ade80", "#f87171", "#f472b6"];

  /** Lines that each have their own times, put on one time axis, with gaps where a line has no value. */
  export function align(lines: { at: number[]; values: number[] }[]): (number | null)[][] {
    const times = [...new Set(lines.flatMap((line) => line.at))].sort((a, b) => a - b);
    const index = new Map(times.map((time, at) => [time, at]));
    const data: (number | null)[][] = [times];
    for (const line of lines) {
      const column: (number | null)[] = new Array(times.length).fill(null);
      line.at.forEach((time, at) => (column[index.get(time)!] = line.values[at]));
      data.push(column);
    }
    return data;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import uPlot from "uplot";

  let {
    labels,
    data,
    version,
    unit = "",
    timeLabel = "seconds",
  }: {
    /** One name a line. */
    labels: string[];
    /** The times, then each line's values, as `align` makes them. */
    data: (number | null)[][];
    /** Changes when the data does, to draw it again. */
    version: number;
    unit?: string;
    timeLabel?: string;
  } = $props();

  let box: HTMLDivElement;
  let plot: uPlot | null = null;
  let drawnLabels = "";

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
  }

  function make(width: number, height: number) {
    plot?.destroy();
    const grid = { stroke: css("--border"), width: 1 };
    const axis = { stroke: css("--muted"), grid, ticks: grid, font: "11px Inter Variable, sans-serif" };
    plot = new uPlot(
      {
        width,
        height,
        legend: { show: true, live: true },
        cursor: { drag: { x: true, y: false } },
        scales: { x: { time: false } },
        axes: [
          { ...axis, label: timeLabel, labelFont: axis.font },
          { ...axis, label: unit, labelFont: axis.font, size: 56 },
        ],
        series: [
          { label: timeLabel, value: (_u, v) => (v == null ? "" : v.toFixed(2)) },
          ...labels.map((label, at) => ({
            label,
            stroke: SERIES_COLORS[at % SERIES_COLORS.length],
            width: 1.5,
            spanGaps: true,
            points: { show: false },
          })),
        ],
      },
      data as uPlot.AlignedData,
      box,
    );
    drawnLabels = labels.join("\u0000");
  }

  onMount(() => {
    const observer = new ResizeObserver(() => {
      const { width, height } = box.getBoundingClientRect();
      if (width < 10 || height < 10) return;
      // Room for the legend under the chart.
      const plotHeight = Math.max(80, height - 40);
      if (!plot) make(width, plotHeight);
      else plot.setSize({ width, height: plotHeight });
    });
    observer.observe(box);
    return () => {
      observer.disconnect();
      plot?.destroy();
      plot = null;
    };
  });

  $effect(() => {
    void version;
    const names = labels.join("\u0000");
    if (!plot) return;
    if (names !== drawnLabels) {
      const { width, height } = box.getBoundingClientRect();
      make(width, Math.max(80, height - 40));
    } else {
      plot.setData(data as uPlot.AlignedData);
    }
  });
</script>

<div class="chart" bind:this={box}></div>

<style>
  .chart {
    width: 100%;
    height: 100%;
    min-height: 120px;
    overflow: hidden;
  }

  .chart :global(.u-legend) {
    font-size: 12px;
    color: var(--text);
  }

  .chart :global(.u-select) {
    background: var(--accent-soft);
  }
</style>
