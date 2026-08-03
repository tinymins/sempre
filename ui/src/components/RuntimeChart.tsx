import { useEffect, useRef } from 'react'
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'

export interface ChartPoint { time: number; download: number; upload: number }

export function RuntimeChart({ points, height = 240 }: { points: ChartPoint[]; height?: number }) {
  const host = useRef<HTMLDivElement>(null)
  const chart = useRef<uPlot | null>(null)
  const initialPoints = useRef(points)

  useEffect(() => {
    if (!host.current) return
    const width = Math.max(host.current.clientWidth, 320)
    chart.current = new uPlot({
      width,
      height,
      cursor: { drag: { x: false, y: false } },
      scales: { x: { time: true }, y: { auto: true } },
      axes: [
        { stroke: '#7a837f', grid: { stroke: 'rgba(122,131,127,.14)' } },
        { stroke: '#7a837f', grid: { stroke: 'rgba(122,131,127,.14)' }, values: (_plot, ticks) => ticks.map(formatRate) },
      ],
      series: [
        {},
        { label: 'Download', stroke: '#0891b2', width: 2, fill: 'rgba(8,145,178,.08)' },
        { label: 'Upload', stroke: '#059669', width: 2, fill: 'rgba(5,150,105,.06)' },
      ],
  }, chartData(initialPoints.current), host.current)
    const observer = new ResizeObserver(([entry]) => chart.current?.setSize({ width: Math.max(Math.floor(entry.contentRect.width), 320), height }))
    observer.observe(host.current)
    return () => { observer.disconnect(); chart.current?.destroy(); chart.current = null }
  }, [height])

  useEffect(() => { chart.current?.setData(chartData(points)) }, [points])
  return <div className="min-w-0 overflow-hidden" ref={host} />
}

function chartData(points: ChartPoint[]): uPlot.AlignedData {
  if (points.length === 0) {
    const now = Math.floor(Date.now() / 1000)
    return [[now - 60, now], [0, 0], [0, 0]]
  }
  return [points.map((point) => point.time), points.map((point) => point.download), points.map((point) => point.upload)]
}

function formatRate(value: number) {
  if (value >= 1024 ** 2) return `${(value / 1024 ** 2).toFixed(1)}M/s`
  if (value >= 1024) return `${(value / 1024).toFixed(0)}K/s`
  return `${value.toFixed(0)}B/s`
}
