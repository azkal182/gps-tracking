import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, waitFor } from '@testing-library/react'
import { HistoryMap } from './history-map'
import type { components } from '@/api/generated'

const mapMock = vi.hoisted(() => ({
  instances: [] as MockMap[],
  worker: vi.fn(),
}))
class MockMap {
  options: { attributionControl?: { compact: boolean } }
  handlers = new Map<string, Array<() => void>>()
  source: { setData: (data: unknown) => void; data: unknown } | null = null
  layers: unknown[] = []
  constructor(options: { attributionControl?: { compact: boolean } }) {
    this.options = options
    mapMock.instances.push(this)
    queueMicrotask(() =>
      this.handlers.get('load')?.forEach((callback) => callback()),
    )
  }
  once(name: string, callback: () => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), callback])
    return this
  }
  on() {
    return this
  }
  isStyleLoaded() {
    return true
  }
  getSource() {
    return this.source
  }
  addSource(_id: string, source: { data: unknown }) {
    this.source = {
      data: source.data,
      setData: (data) => {
        this.source!.data = data
      },
    }
  }
  addLayer(layer: unknown) {
    this.layers.push(layer)
  }
  jumpTo() {
    return this
  }
  fitBounds() {
    return this
  }
  remove() {
    return this
  }
}
vi.mock('maplibre-gl', () => ({
  Map: MockMap,
  LngLatBounds: class {
    extend() {
      return this
    }
  },
  setWorkerUrl: mapMock.worker,
}))

const position = (
  id: string,
  latitude: number,
  longitude: number,
): components['schemas']['Position'] => ({
  id,
  tenant_id: 'tenant-map',
  device_id: 'device-map',
  message_id: id,
  latitude,
  longitude,
  altitude_m: null,
  speed_mps: null,
  heading_deg: null,
  accuracy_m: null,
  recorded_at: '2026-10-20T00:00:00Z',
  received_at: '2026-10-20T00:00:01Z',
  attributes: {},
})

afterEach(() => {
  mapMock.instances.length = 0
  vi.unstubAllEnvs()
})

describe('history map', () => {
  it('renders local position overlays as a line and points with visible attribution', async () => {
    vi.stubEnv('VITE_MAP_STYLE_URL', 'https://tiles.example.invalid/style.json')
    const positions = [
      position('newer', -6.2, 106.8),
      position('older', -6.3, 106.9),
    ]
    render(<HistoryMap positions={positions} />)
    await waitFor(() => expect(mapMock.instances[0]?.layers).toHaveLength(2))
    const map = mapMock.instances[0]!
    expect(map.options.attributionControl).toEqual({ compact: false })
    const features = (
      map.source?.data as { features: Array<{ geometry: { type: string } }> }
    ).features
    expect(features.map((feature) => feature.geometry.type)).toEqual([
      'LineString',
      'Point',
      'Point',
    ])
  })
})
