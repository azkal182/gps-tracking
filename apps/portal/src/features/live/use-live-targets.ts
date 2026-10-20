import { useEffect, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { portalApi, withPortalMutation } from '@/api/client'
import { organizationQueryKey } from '@/portal-context'
import type { components } from '@/api/generated'
import {
  applyLiveEvent,
  mergeSnapshotRows,
  type LiveTarget,
  type Target,
  type WsServerEvent,
} from './live-state'

type TargetPage = components['schemas']['TargetPage']
type TargetLocation = components['schemas']['TargetLocation']

const locationKey = (organizationId: string, targets: Target[]) =>
  organizationQueryKey(
    organizationId,
    'live-target-locations',
    ...targets.map((target) => target.id),
  )

async function mapConcurrent<T, R>(
  items: T[],
  concurrency: number,
  work: (item: T) => Promise<R>,
) {
  const results = new Array<R>(items.length)
  let next = 0
  await Promise.all(
    Array.from({ length: Math.min(concurrency, items.length) }, async () => {
      while (next < items.length) {
        const index = next++
        results[index] = await work(items[index]!)
      }
    }),
  )
  return results
}

export function useLiveTargets(organizationId: string | null) {
  const queryClient = useQueryClient()
  const [connection, setConnection] = useState<
    'connecting' | 'connected' | 'reconnecting' | 'unavailable'
  >('connecting')
  const [socketError, setSocketError] = useState<string | null>(null)
  const targetsQuery = useQuery({
    queryKey: organizationQueryKey(organizationId ?? 'none', 'live-targets'),
    enabled: !!organizationId,
    queryFn: async ({ signal }) => {
      const result: Target[] = []
      let cursor: string | undefined
      do {
        const { data, error } = await portalApi.GET('/targets', {
          params: {
            header: { 'X-Organization-Id': organizationId! },
            query: { limit: 100, ...(cursor ? { cursor } : {}) },
          },
          signal,
        })
        if (error || !data)
          throw error ?? new Error('Daftar target tidak tersedia')
        result.push(...(data as TargetPage).data)
        cursor = (data as TargetPage).next_cursor ?? undefined
      } while (cursor)
      return result
    },
  })
  const targets = targetsQuery.data ?? []
  const locationsQuery = useQuery({
    queryKey: locationKey(organizationId ?? 'none', targets),
    enabled: !!organizationId && !!targetsQuery.data,
    queryFn: async ({ signal }) =>
      mapConcurrent(targets, 8, async (target): Promise<LiveTarget | null> => {
        const { data, error, response } = await portalApi.GET(
          '/targets/{target_id}/location',
          {
            params: {
              header: { 'X-Organization-Id': organizationId! },
              path: { target_id: target.id },
            },
            signal,
          },
        )
        if (response.status === 403) return null
        if (error || !data)
          throw error ?? new Error('Status lokasi target tidak tersedia')
        const location = data as TargetLocation
        if (
          location.tenant_id !== organizationId ||
          location.target_id !== target.id
        )
          throw new Error('Snapshot lokasi tidak cocok dengan organisasi aktif')
        return { target, location }
      }).then((results) => {
        const incoming = results.filter(
          (row): row is LiveTarget => row !== null,
        )
        return mergeSnapshotRows(
          queryClient.getQueryData<LiveTarget[]>(
            locationKey(organizationId!, targets),
          ) ?? [],
          incoming,
        )
      }),
  })
  const rows = locationsQuery.data ?? []
  const targetIds = targets.map((target) => target.id)
  const authorizedTargetIdsSignature = rows
    .map((row) => row.target.id)
    .join(',')
  const locationsLoaded = locationsQuery.data !== undefined

  useEffect(() => {
    if (
      !organizationId ||
      !targetsQuery.data ||
      !locationsLoaded ||
      !authorizedTargetIdsSignature
    ) {
      return
    }
    const subscriptionTargetIds = authorizedTargetIdsSignature.split(',')
    let disposed = false
    let socket: WebSocket | null = null
    let retryTimer: ReturnType<typeof setTimeout> | null = null
    let attempt = 0
    const currentLocationKey = locationKey(organizationId, targetsQuery.data)
    const refreshSnapshot = () => {
      void queryClient.invalidateQueries({ queryKey: currentLocationKey })
    }
    const connect = async () => {
      if (disposed) return
      setConnection(attempt === 0 ? 'connecting' : 'reconnecting')
      try {
        const { data, error, response } = await withPortalMutation(() =>
          portalApi.POST('/realtime/tickets', {
            params: { header: { 'X-Organization-Id': organizationId } },
            body: {},
          }),
        )
        if (response.status === 401 || response.status === 403) {
          setConnection('unavailable')
          setSocketError(
            'Akses realtime tidak diberikan untuk organisasi ini; snapshot REST tetap tersedia.',
          )
          return
        }
        if (error || !data)
          throw error ?? new Error('Tiket realtime tidak tersedia')
        if (disposed) return
        const apiBase = new URL(
          import.meta.env.VITE_API_BASE_URL ?? '/v1',
          window.location.origin,
        )
        const socketUrl = new URL(
          `${apiBase.pathname.replace(/\/$/, '')}/ws/tracking`,
          apiBase.origin,
        )
        socketUrl.protocol = socketUrl.protocol === 'https:' ? 'wss:' : 'ws:'
        socketUrl.searchParams.set('ticket', data.ticket)
        socket = new WebSocket(socketUrl)
        socket.onopen = () => {
          if (disposed || socket?.readyState !== WebSocket.OPEN) return
          socket.send(
            JSON.stringify({
              type: 'subscribe',
              request_id: crypto.randomUUID(),
              target_ids: subscriptionTargetIds,
              device_ids: [],
            }),
          )
        }
        socket.onmessage = (message) => {
          if (disposed) return
          let event: WsServerEvent | { type: 'subscribed' | 'error' }
          try {
            event = JSON.parse(String(message.data)) as typeof event
          } catch {
            return
          }
          if (event.type === 'subscribed') {
            attempt = 0
            setSocketError(null)
            setConnection('connected')
            refreshSnapshot()
          } else if (event.type === 'error') {
            setSocketError(
              'Subscription realtime ditolak. Periksa izin target di organisasi ini.',
            )
          } else if (
            event.type === 'session.started' ||
            event.type === 'session.ended' ||
            event.type === 'tracking.status_changed'
          ) {
            refreshSnapshot()
          } else {
            queryClient.setQueryData<LiveTarget[]>(
              currentLocationKey,
              (current) => {
                if (!current) return current
                return applyLiveEvent(current, event as WsServerEvent)
              },
            )
          }
        }
        socket.onerror = () =>
          setSocketError(
            'Koneksi realtime terganggu; status REST tetap tersedia.',
          )
        socket.onclose = () => {
          if (disposed) return
          attempt += 1
          setConnection('reconnecting')
          refreshSnapshot()
          retryTimer = setTimeout(
            () => void connect(),
            Math.min(30_000, 1_000 * 2 ** Math.min(attempt - 1, 5)),
          )
        }
      } catch {
        if (disposed) return
        attempt += 1
        setConnection('reconnecting')
        setSocketError(
          'Realtime belum tersedia; mencoba menyambungkan kembali.',
        )
        retryTimer = setTimeout(
          () => void connect(),
          Math.min(30_000, 1_000 * 2 ** Math.min(attempt - 1, 5)),
        )
      }
    }
    void connect()
    return () => {
      disposed = true
      if (retryTimer) clearTimeout(retryTimer)
      socket?.close()
    }
  }, [
    organizationId,
    targetsQuery.data,
    locationsLoaded,
    authorizedTargetIdsSignature,
    queryClient,
  ])

  return {
    rows,
    isLoading:
      targetsQuery.isLoading ||
      (targetsQuery.data !== undefined && locationsQuery.isLoading),
    error: targetsQuery.error ?? locationsQuery.error,
    connection:
      !organizationId ||
      (targetsQuery.data !== undefined && targetIds.length === 0)
        ? 'unavailable'
        : connection,
    socketError,
    restrictedCount: Math.max(0, targetIds.length - rows.length),
    refresh: () => {
      void targetsQuery.refetch()
      void locationsQuery.refetch()
    },
  }
}
