import { describe, expect, it } from 'vitest'
import {
  applyLiveEvent,
  mergeSnapshotRows,
  type LiveTarget,
  type WsServerEvent,
} from './live-state'

const row = (overrides: Partial<LiveTarget['location']> = {}): LiveTarget => ({
  target: { id: 'target-1', name: 'Tracker 1' } as LiveTarget['target'],
  location: {
    target_id: 'target-1',
    tracking_status: 'active',
    connectivity_status: 'online',
    position_status: 'fresh',
    session_id: 'session-1',
    device_id: 'device-1',
    tenant_id: 'tenant-1',
    observed_at: '2026-10-20T01:00:00Z',
    last_contact_at: '2026-10-20T01:00:00Z',
    position: {
      id: '12',
      latitude: -6.2,
      longitude: 106.8,
      recorded_at: '2026-10-20T01:00:00Z',
      received_at: '2026-10-20T01:00:01Z',
      speed_mps: null,
      heading_deg: null,
    },
    ...overrides,
  } as LiveTarget['location'],
})

describe('live map state', () => {
  it('accepts heartbeat connectivity without changing position or freshness', () => {
    const initial = row()
    const event = {
      type: 'device.connectivity_changed',
      event_id: 'event-1',
      emitted_at: '2026-10-20T01:01:00Z',
      observed_at: '2026-10-20T01:01:00Z',
      data: {
        device_id: 'device-1',
        connectivity_status: 'online',
        last_contact_at: '2026-10-20T01:01:00Z',
      },
    } as WsServerEvent
    const updated = applyLiveEvent([initial], event)[0]!
    expect(updated.location.connectivity_status).toBe('online')
    expect(updated.location.position).toEqual(initial.location.position)
    expect(updated.location.position_status).toBe('fresh')
  })

  it('removes a marker when an authorized session status says no position remains', () => {
    const initial = row()
    const event = {
      type: 'position.status_changed',
      event_id: 'event-no-position',
      emitted_at: '2026-10-20T01:01:00Z',
      observed_at: '2026-10-20T01:01:00Z',
      data: {
        device_id: 'device-1',
        target_id: 'target-1',
        session_id: 'session-1',
        position_status: 'no_position',
      },
    } as WsServerEvent
    const updated = applyLiveEvent([initial], event)[0]!
    expect(updated.location.position_status).toBe('no_position')
    expect(updated.location.position).toBeNull()
  })

  it('ignores events for a prior session and positions older than the current sample', () => {
    const initial = row()
    const wrongSession = {
      type: 'position.updated',
      event_id: 'event-1',
      emitted_at: '2026-10-20T01:02:00Z',
      data: {
        target_id: 'target-1',
        device_id: 'device-1',
        session_id: 'old-session',
        position: {
          id: '99',
          latitude: 1,
          longitude: 1,
          recorded_at: '2026-10-20T01:02:00Z',
          received_at: '2026-10-20T01:02:01Z',
        },
      },
    } as WsServerEvent
    const latePosition = {
      ...wrongSession,
      data: {
        ...wrongSession.data,
        session_id: 'session-1',
        position: {
          id: '99',
          latitude: 1,
          longitude: 1,
          recorded_at: '2026-10-19T23:59:00Z',
          received_at: '2026-10-20T01:02:01Z',
        },
      },
    } as WsServerEvent
    expect(
      applyLiveEvent([initial], wrongSession)[0]!.location.position,
    ).toEqual(initial.location.position)
    expect(
      applyLiveEvent([initial], latePosition)[0]!.location.position,
    ).toEqual(initial.location.position)
  })

  it('keeps an event newer than a reconnect snapshot in the same session', () => {
    const latest = row({
      observed_at: '2026-10-20T01:03:00Z',
      position: {
        id: '14',
        latitude: -6.3,
        longitude: 106.9,
        recorded_at: '2026-10-20T01:02:59Z',
        received_at: '2026-10-20T01:03:00Z',
        speed_mps: null,
        heading_deg: null,
      },
    })
    const oldSnapshot = row({ observed_at: '2026-10-20T01:02:00Z' })
    expect(
      mergeSnapshotRows([latest], [oldSnapshot])[0]!.location.position,
    ).toEqual(latest.location.position)
  })
})
