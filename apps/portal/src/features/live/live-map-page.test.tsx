import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { App } from '@/App'

const mapMocks = vi.hoisted(() => ({
  failProvider: false,
  denyLocation: false,
  maps: [] as MockMap[],
  markers: [] as MockMarker[],
}))
class MockMap {
  handlers = new Map<string, Array<() => void>>()
  options: { attributionControl?: { compact: boolean } }
  constructor(options: { attributionControl?: { compact: boolean } }) {
    this.options = options
    mapMocks.maps.push(this)
    queueMicrotask(() => this.emit(mapMocks.failProvider ? 'error' : 'load'))
  }
  once(name: string, callback: () => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), callback])
    return this
  }
  on(name: string, callback: () => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), callback])
    return this
  }
  emit(name: string) {
    this.handlers.get(name)?.forEach((callback) => callback())
  }
  remove() {
    return this
  }
  jumpTo() {
    return this
  }
  fitBounds() {
    return this
  }
}
class MockMarker {
  lngLat: [number, number] = [0, 0]
  private readonly options: { element: HTMLElement }
  constructor(options: { element: HTMLElement }) {
    this.options = options
    mapMocks.markers.push(this)
  }
  setLngLat(value: [number, number]) {
    this.lngLat = value
    return this
  }
  addTo() {
    return this
  }
  getElement() {
    return this.options.element
  }
  remove() {
    return this
  }
}
vi.mock('maplibre-gl', () => ({
  Map: MockMap,
  Marker: MockMarker,
  LngLatBounds: class {
    extend() {
      return this
    }
  },
  setWorkerUrl: vi.fn(),
}))

const tenantA = 'tenant-live-a'
const tenantB = 'tenant-live-b'
const targetA = 'target-live-a'
const targetB = 'target-live-b'
const organizations = [
  {
    id: tenantA,
    tenant_id: tenantA,
    name: 'Operations A',
    status: 'active',
    created_at: '2026-10-20T00:00:00Z',
    updated_at: '2026-10-20T00:00:00Z',
  },
  {
    id: tenantB,
    tenant_id: tenantB,
    name: 'Operations B',
    status: 'active',
    created_at: '2026-10-20T00:00:00Z',
    updated_at: '2026-10-20T00:00:00Z',
  },
]
const userData = {
  user: {
    id: 'user-live',
    email: 'operator@example.invalid',
    display_name: 'Operator',
    status: 'active',
    created_at: '2026-10-20T00:00:00Z',
    updated_at: '2026-10-20T00:00:00Z',
  },
  mfa_required: true,
  mfa_enabled: true,
  memberships: organizations.map((org) => ({
    id: `membership-${org.id}`,
    tenant_id: org.id,
    user_id: 'user-live',
    role: 'admin',
    status: 'active',
    created_at: org.created_at,
    updated_at: org.updated_at,
  })),
}
const sockets: MockWebSocket[] = []
class MockWebSocket {
  static OPEN = 1
  static CLOSED = 3
  readyState = MockWebSocket.OPEN
  onopen: (() => void) | null = null
  onmessage: ((event: { data: string }) => void) | null = null
  onerror: (() => void) | null = null
  onclose: (() => void) | null = null
  sent: string[] = []
  readonly url: string | URL
  constructor(url: string | URL) {
    this.url = url
    sockets.push(this)
    queueMicrotask(() => this.onopen?.())
  }
  send(message: string) {
    this.sent.push(message)
    const parsed = JSON.parse(message) as {
      request_id: string
      target_ids: string[]
    }
    queueMicrotask(() =>
      this.onmessage?.({
        data: JSON.stringify({
          type: 'subscribed',
          request_id: parsed.request_id,
          target_ids: parsed.target_ids,
          device_ids: [],
        }),
      }),
    )
  }
  close() {
    this.readyState = MockWebSocket.CLOSED
    this.onclose?.()
  }
  emit(value: unknown) {
    this.onmessage?.({ data: JSON.stringify(value) })
  }
}

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}
function requestUrl(input: RequestInfo | URL) {
  return input instanceof Request ? input.url : String(input)
}
function requestHeader(input: RequestInfo | URL, name: string) {
  return input instanceof Request ? input.headers.get(name) : null
}
function targetFor(tenantId: string) {
  const id = tenantId === tenantA ? targetA : targetB
  return {
    id,
    tenant_id: tenantId,
    name: tenantId === tenantA ? 'Route A' : 'Route B',
    external_ref: null,
    metadata: {},
    created_at: '2026-10-20T00:00:00Z',
    updated_at: '2026-10-20T00:00:00Z',
  }
}
function locationFor(tenantId: string) {
  const target = targetFor(tenantId)
  return {
    target_id: target.id,
    tracking_status: 'active',
    connectivity_status: 'offline',
    position_status: 'stale',
    session_id: `session-${target.id}`,
    device_id: `device-${target.id}`,
    tenant_id: tenantId,
    observed_at: '2026-10-20T01:00:00Z',
    last_contact_at: '2026-10-20T00:59:00Z',
    position: {
      id: '12',
      latitude: -6.2,
      longitude: 106.8,
      recorded_at: '2026-10-20T00:59:00Z',
      received_at: '2026-10-20T00:59:01Z',
      speed_mps: null,
      heading_deg: null,
    },
  }
}

beforeEach(() => {
  mapMocks.failProvider = false
  mapMocks.denyLocation = false
  mapMocks.maps.length = 0
  mapMocks.markers.length = 0
  sockets.length = 0
  window.sessionStorage.clear()
  window.localStorage.clear()
  vi.stubEnv('VITE_MAP_STYLE_URL', 'https://tiles.example.invalid/style.json')
  vi.stubGlobal('WebSocket', MockWebSocket)
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = requestUrl(input)
      const tenant = requestHeader(input, 'X-Organization-Id') ?? tenantA
      if (url.includes('/auth/me')) return json(userData)
      if (url.includes('/organizations'))
        return json({ data: organizations, next_cursor: null })
      if (url.includes('/auth/csrf'))
        return json({ csrf_token: 'csrf-synthetic' })
      if (url.includes('/realtime/tickets'))
        return json({
          ticket: `ticket-${tenant}`,
          expires_at: '2026-10-20T01:01:00Z',
        })
      if (url.includes('/targets?') || url.endsWith('/targets'))
        return json({ data: [targetFor(tenant)], next_cursor: null })
      if (url.includes('/location')) {
        if (mapMocks.denyLocation)
          return json(
            {
              error: {
                code: 'FORBIDDEN',
                message: 'Forbidden',
                request_id: 'synthetic',
              },
            },
            403,
          )
        return json(locationFor(tenant))
      }
      return json({})
    }),
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

describe('live map journey', () => {
  it('keeps status available when the configured map provider fails', async () => {
    mapMocks.failProvider = true
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Peta live' }))
    expect(
      await screen.findByText('Peta tidak dapat dimuat'),
    ).toBeInTheDocument()
    expect(await screen.findByText('Route A')).toBeInTheDocument()
    expect(screen.getByText('Offline')).toBeInTheDocument()
    expect(screen.getByText('Posisi lama')).toBeInTheDocument()
    expect(mapMocks.maps[0]?.options.attributionControl).toEqual({
      compact: false,
    })
  })

  it('does not subscribe to or display a target without a location grant', async () => {
    mapMocks.denyLocation = true
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Peta live' }))
    expect(
      await screen.findByText(/Lokasi untuk 1 target tidak ditampilkan/),
    ).toBeInTheDocument()
    expect(screen.queryByText('Route A')).not.toBeInTheDocument()
    expect(sockets).toHaveLength(0)
  })

  it('reconnects under the selected tenant and preserves a stale marker on heartbeat', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Peta live' }))
    expect(await screen.findByText('Route A')).toBeInTheDocument()
    const marker = await waitFor(() => {
      expect(mapMocks.markers).toHaveLength(1)
      expect(sockets).toHaveLength(1)
      expect(JSON.parse(sockets[0]!.sent[0]!).target_ids).toEqual([targetA])
      expect(mapMocks.markers[0]!.getElement().classList).toContain(
        'live-map-marker--stale',
      )
      return mapMocks.markers[0]!
    })
    const markerLocation = marker.lngLat

    await user.selectOptions(screen.getByLabelText('Organisasi aktif'), tenantB)
    expect(await screen.findByText('Route B')).toBeInTheDocument()
    await waitFor(() => {
      expect(sockets).toHaveLength(2)
      expect(sockets[0]!.readyState).toBe(MockWebSocket.CLOSED)
      expect(JSON.parse(sockets[1]!.sent[0]!).target_ids).toEqual([targetB])
      expect(decodeURIComponent(String(sockets[1]!.url))).toContain(
        `ticket-${tenantB}`,
      )
      expect(screen.queryByText('Route A')).not.toBeInTheDocument()
    })
    const activeSocket = sockets[1]!
    const markerForB = mapMocks.markers.at(-1)!
    await act(async () =>
      activeSocket.emit({
        type: 'device.connectivity_changed',
        event_id: 'heartbeat-1',
        emitted_at: '2026-10-20T01:02:00Z',
        observed_at: '2026-10-20T01:02:00Z',
        data: {
          device_id: `device-${targetB}`,
          connectivity_status: 'online',
          last_contact_at: '2026-10-20T01:02:00Z',
        },
      }),
    )
    expect(await screen.findByText('Online')).toBeInTheDocument()
    expect(markerForB.getElement().getAttribute('aria-label')).toBe(
      'Route B, posisi lama',
    )
    expect(markerForB.getElement().classList).toContain(
      'live-map-marker--stale',
    )
    expect(marker.lngLat).toEqual(markerLocation)
    expect(markerForB.lngLat).toEqual(markerLocation)
  })
})
