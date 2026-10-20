import { useEffect, useMemo, useRef, useState } from 'react'
import type { Map as MapInstance, GeoJSONSource } from 'maplibre-gl'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url'
import 'maplibre-gl/dist/maplibre-gl.css'
import type { components } from '@/api/generated'
import { getMapStyleUrl } from '@/features/live/map-config'
import { visibleMapPositions } from './history-utils'

type Position = components['schemas']['Position']

export function HistoryMap({ positions }: { positions: Position[] }) {
  const container = useRef<HTMLDivElement>(null)
  const map = useRef<MapInstance | null>(null)
  const maplibre = useRef<typeof import('maplibre-gl') | null>(null)
  const [ready, setReady] = useState(false)
  const [error, setError] = useState(false)
  const style = useMemo(
    () =>
      getMapStyleUrl({
        VITE_MAP_STYLE_URL: import.meta.env.VITE_MAP_STYLE_URL,
        VITE_MAPTILER_KEY: import.meta.env.VITE_MAPTILER_KEY,
      }),
    [],
  )
  const visible = useMemo(() => visibleMapPositions(positions), [positions])

  useEffect(() => {
    if (!style || !container.current) return
    let disposed = false
    let instance: MapInstance | null = null
    void import('maplibre-gl')
      .then((module) => {
        if (disposed || !container.current) return
        module.setWorkerUrl(workerUrl)
        maplibre.current = module
        instance = new module.Map({
          container: container.current,
          style,
          center: [117, -2],
          zoom: 3,
          attributionControl: { compact: false },
        })
        map.current = instance
        instance.once('load', () => setReady(true))
        instance.on('error', () => {
          if (!disposed) {
            setError(true)
            instance?.remove()
            map.current = null
            setReady(false)
          }
        })
      })
      .catch(() => setError(true))
    return () => {
      disposed = true
      instance?.remove()
      if (map.current === instance) map.current = null
      maplibre.current = null
    }
  }, [style])

  useEffect(() => {
    const instance = map.current
    const module = maplibre.current
    if (!instance || !module || !ready || error) return
    const features = visible.map((position) => ({
      type: 'Feature' as const,
      properties: { recorded_at: position.recorded_at },
      geometry: {
        type: 'Point' as const,
        coordinates: [Number(position.longitude), Number(position.latitude)],
      },
    }))
    const coordinates = features.map(
      (feature) => feature.geometry.coordinates as [number, number],
    )
    const geojson = {
      type: 'FeatureCollection' as const,
      features: [
        ...(coordinates.length > 1
          ? [
              {
                type: 'Feature' as const,
                properties: {},
                geometry: { type: 'LineString' as const, coordinates },
              },
            ]
          : []),
        ...features,
      ],
    }
    const apply = () => {
      const source = instance!.getSource('history') as GeoJSONSource | undefined
      if (source) {
        source.setData(geojson)
      } else {
        instance!.addSource('history', { type: 'geojson', data: geojson })
        instance!.addLayer({
          id: 'history-line',
          type: 'line',
          source: 'history',
          filter: ['==', ['geometry-type'], 'LineString'],
          paint: {
            'line-color': '#2563eb',
            'line-width': 3,
            'line-opacity': 0.75,
          },
        })
        instance!.addLayer({
          id: 'history-points',
          type: 'circle',
          source: 'history',
          filter: ['==', ['geometry-type'], 'Point'],
          paint: {
            'circle-radius': 4,
            'circle-color': '#dc2626',
            'circle-stroke-width': 1,
            'circle-stroke-color': '#fff',
          },
        })
      }
      if (coordinates.length === 1)
        instance!.jumpTo({ center: coordinates[0], zoom: 13 })
      else if (coordinates.length > 1) {
        const bounds = new module!.LngLatBounds()
        coordinates.forEach((coordinate) => bounds.extend(coordinate))
        instance!.fitBounds(bounds, { padding: 48, maxZoom: 13 })
      }
    }
    if (instance.isStyleLoaded()) apply()
    else instance.once('load', apply)
  }, [visible, ready, error])

  if (!style || error)
    return (
      <div
        className="bg-muted/30 text-muted-foreground grid min-h-[24rem] place-items-center rounded-xl border p-8 text-center"
        role="status"
      >
        <div>
          <p className="text-foreground font-medium">
            {error ? 'Peta tidak dapat dimuat' : 'Peta belum dikonfigurasi'}
          </p>
          <p className="mt-1 max-w-sm text-sm">
            {error
              ? 'Tile provider bermasalah. Tabel histori tetap dapat digunakan.'
              : 'Atur VITE_MAP_STYLE_URL atau VITE_MAPTILER_KEY untuk menampilkan peta.'}
          </p>
        </div>
      </div>
    )
  return (
    <div
      aria-label="Peta histori posisi"
      className="min-h-[24rem] overflow-hidden rounded-xl border"
      ref={container}
    />
  )
}
