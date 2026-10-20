import { useEffect, useMemo, useRef, useState } from 'react'
import type { Map as MapInstance, Marker as MarkerInstance } from 'maplibre-gl'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url'
import 'maplibre-gl/dist/maplibre-gl.css'
import type { LiveTarget } from './live-state'
import { getMapStyleUrl } from './map-config'

export function LiveMap({ rows }: { rows: LiveTarget[] }) {
  const container = useRef<HTMLDivElement>(null)
  const map = useRef<MapInstance | null>(null)
  const maplibreModule = useRef<typeof import('maplibre-gl') | null>(null)
  const markers = useRef(new globalThis.Map<string, MarkerInstance>())
  const hasFitted = useRef(false)
  const [providerError, setProviderError] = useState(false)
  const [mapReady, setMapReady] = useState(false)
  const style = useMemo(
    () =>
      getMapStyleUrl({
        VITE_MAP_STYLE_URL: import.meta.env.VITE_MAP_STYLE_URL,
        VITE_MAPTILER_KEY: import.meta.env.VITE_MAPTILER_KEY,
      }),
    [],
  )
  const positionedRows = rows.filter(
    (row) =>
      row.location.tracking_status === 'active' &&
      row.location.session_id &&
      row.location.position,
  )

  useEffect(() => {
    if (!container.current || !style) return
    const mapMarkers = markers.current
    let disposed = false
    let instance: MapInstance | null = null
    void import('maplibre-gl')
      .then((module) => {
        if (disposed || !container.current) return
        module.setWorkerUrl(workerUrl)
        maplibreModule.current = module
        instance = new module.Map({
          container: container.current,
          style,
          center: [117, -2],
          zoom: 3,
          attributionControl: { compact: false },
        })
        map.current = instance
        instance.once('load', () => setMapReady(true))
        instance.on('error', () => {
          setProviderError(true)
          instance?.remove()
          map.current = null
        })
      })
      .catch(() => setProviderError(true))
    return () => {
      disposed = true
      mapMarkers.forEach((marker) => marker.remove())
      mapMarkers.clear()
      instance?.remove()
      if (map.current === instance) map.current = null
      maplibreModule.current = null
    }
  }, [style])

  useEffect(() => {
    const instance = map.current
    const module = maplibreModule.current
    if (!instance || !module || !mapReady || providerError) return
    const activeIds = new Set(positionedRows.map((row) => row.target.id))
    for (const [targetId, marker] of markers.current) {
      if (!activeIds.has(targetId)) {
        marker.remove()
        markers.current.delete(targetId)
      }
    }
    for (const row of positionedRows) {
      const position = row.location.position!
      let marker = markers.current.get(row.target.id)
      if (!marker) {
        const element = document.createElement('div')
        element.className = 'live-map-marker'
        element.setAttribute('aria-label', row.target.name)
        element.title = row.target.name
        marker = new module.Marker({ element, anchor: 'center' })
          .setLngLat([position.longitude, position.latitude])
          .addTo(instance)
        markers.current.set(row.target.id, marker)
      }
      marker.setLngLat([position.longitude, position.latitude])
      const element = marker.getElement()
      element.classList.toggle(
        'live-map-marker--stale',
        row.location.position_status === 'stale',
      )
      element.setAttribute(
        'aria-label',
        `${row.target.name}, posisi ${row.location.position_status === 'stale' ? 'lama' : 'terbaru'}`,
      )
    }
    if (!hasFitted.current && positionedRows.length > 0) {
      if (positionedRows.length === 1) {
        const position = positionedRows[0]!.location.position!
        instance.jumpTo({
          center: [position.longitude, position.latitude],
          zoom: 13,
        })
      } else {
        const bounds = new module.LngLatBounds()
        positionedRows.forEach((row) =>
          bounds.extend([
            row.location.position!.longitude,
            row.location.position!.latitude,
          ]),
        )
        instance.fitBounds(bounds, { padding: 48, maxZoom: 13 })
      }
      hasFitted.current = true
    }
  }, [positionedRows, providerError, mapReady])

  if (!style || providerError) {
    return (
      <div
        className="bg-muted/30 text-muted-foreground grid min-h-[24rem] place-items-center rounded-xl border p-8 text-center"
        role="status"
      >
        <div>
          <p className="text-foreground font-medium">
            {providerError
              ? 'Peta tidak dapat dimuat'
              : 'Peta belum dikonfigurasi'}
          </p>
          <p className="mt-1 max-w-sm text-sm">
            {providerError
              ? 'Tile provider sedang bermasalah. Daftar status target tetap tersedia di bawah.'
              : 'Atur VITE_MAP_STYLE_URL atau VITE_MAPTILER_KEY untuk menampilkan peta.'}
          </p>
        </div>
      </div>
    )
  }

  return (
    <div
      aria-label="Peta posisi target pada sesi aktif"
      className="min-h-[24rem] overflow-hidden rounded-xl border"
      ref={container}
    />
  )
}
