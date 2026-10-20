import type { components } from '@/api/generated'

export type Target = components['schemas']['Target']
export type TargetLocation = components['schemas']['TargetLocation']
export type Position = components['schemas']['LatestPosition']
export type LiveTarget = { target: Target; location: TargetLocation }
export type WsServerEvent = components['schemas']['WsServerEvent']

function positionIsNewer(
  candidate: Pick<Position, 'id' | 'recorded_at'>,
  current: Pick<Position, 'id' | 'recorded_at'> | null,
) {
  if (!current) return true
  const candidateTime = Date.parse(candidate.recorded_at)
  const currentTime = Date.parse(current.recorded_at)
  if (candidateTime !== currentTime) return candidateTime > currentTime
  try {
    return BigInt(candidate.id) > BigInt(current.id)
  } catch {
    return candidate.id > current.id
  }
}

export function applyLiveEvent(
  rows: LiveTarget[],
  event: WsServerEvent,
): LiveTarget[] {
  if (event.type === 'position.updated') {
    const {
      target_id: targetId,
      session_id: sessionId,
      device_id: deviceId,
      position: eventPosition,
    } = event.data
    if (!targetId || !sessionId) return rows
    return rows.map((row) => {
      if (
        row.target.id !== targetId ||
        row.location.session_id !== sessionId ||
        row.location.device_id !== deviceId ||
        !positionIsNewer(eventPosition, row.location.position)
      )
        return row
      const position: Position = {
        id: eventPosition.id,
        latitude: eventPosition.latitude,
        longitude: eventPosition.longitude,
        recorded_at: eventPosition.recorded_at,
        received_at: eventPosition.received_at,
        speed_mps: eventPosition.speed_mps ?? null,
        heading_deg: eventPosition.heading_deg ?? null,
      }
      return {
        ...row,
        location: {
          ...row.location,
          position,
          position_status: 'fresh',
          observed_at: event.emitted_at,
        },
      }
    })
  }

  if (event.type === 'device.connectivity_changed') {
    return rows.map((row) => {
      if (row.location.device_id !== event.data.device_id) return row
      if (Date.parse(event.observed_at) <= Date.parse(row.location.observed_at))
        return row
      return {
        ...row,
        location: {
          ...row.location,
          connectivity_status: event.data.connectivity_status,
          last_contact_at: event.data.last_contact_at,
          observed_at: event.observed_at,
        },
      }
    })
  }

  if (event.type === 'position.status_changed') {
    if (!event.data.target_id || !event.data.session_id) return rows
    return rows.map((row) => {
      if (
        row.target.id !== event.data.target_id ||
        row.location.session_id !== event.data.session_id ||
        Date.parse(event.observed_at) <= Date.parse(row.location.observed_at)
      )
        return row
      return {
        ...row,
        location: {
          ...row.location,
          position_status: event.data.position_status,
          ...(event.data.position_status === 'no_position'
            ? { position: null }
            : {}),
          observed_at: event.observed_at,
        },
      }
    })
  }

  return rows
}

export function mergeSnapshotRows(
  current: LiveTarget[],
  incoming: LiveTarget[],
): LiveTarget[] {
  const currentByTarget = new Map(current.map((row) => [row.target.id, row]))
  return incoming.map((snapshot) => {
    const previous = currentByTarget.get(snapshot.target.id)
    if (
      !previous ||
      previous.location.session_id !== snapshot.location.session_id
    )
      return snapshot
    const currentPosition = previous.location.position
    const snapshotPosition = snapshot.location.position
    const position =
      currentPosition && positionIsNewer(currentPosition, snapshotPosition)
        ? currentPosition
        : snapshotPosition
    const currentStatusIsNewer =
      Date.parse(previous.location.observed_at) >
      Date.parse(snapshot.location.observed_at)
    return {
      ...snapshot,
      location: {
        ...snapshot.location,
        ...(currentStatusIsNewer
          ? {
              connectivity_status: previous.location.connectivity_status,
              position_status: previous.location.position_status,
              last_contact_at: previous.location.last_contact_at,
              observed_at: previous.location.observed_at,
            }
          : {}),
        position:
          currentStatusIsNewer &&
          previous.location.position_status === 'no_position'
            ? null
            : position,
      },
    }
  })
}
