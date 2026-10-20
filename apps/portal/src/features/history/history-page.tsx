import { useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { useMemo, useState, type FormEvent } from 'react'
import type { components } from '@/api/generated'
import { portalApi } from '@/api/client'
import {
  usePortalAuth,
  usePortalTenant,
  organizationQueryKey,
} from '@/portal-context'
import { HistoryAuditPanel } from './history-audit-panel'
import { HistoryFilterForm } from './history-filter-form'
import { HistoryPositionPanel } from './history-position-panel'
import {
  browserTimeZone,
  defaultHistoryRange,
  HISTORY_MAP_PAGE_SIZE,
  HISTORY_MAP_POINT_LIMIT,
  validateHistoryRange,
  type ApiHistoryRange,
  type LocalHistoryRange,
} from './history-utils'

type Device = components['schemas']['Device']
type Session = components['schemas']['TrackingSession']
type AuditEvent = components['schemas']['AuditEvent']
type Page<T> = { data: T[]; next_cursor: string | null }
type SourceType = 'device' | 'session'

function assertTenantPage<T extends { tenant_id: string }>(
  page: Page<T>,
  tenantId: string,
): Page<T> {
  if (page.data.some((row) => row.tenant_id !== tenantId))
    throw new Error('Respons histori tidak cocok dengan organisasi aktif.')
  return page
}

export function HistoryPage() {
  const { organizationId } = usePortalTenant()
  return (
    <HistoryContent
      key={organizationId ?? 'none'}
      organizationId={organizationId}
    />
  )
}

function HistoryContent({ organizationId }: { organizationId: string | null }) {
  const auth = usePortalAuth()
  const { memberships } = usePortalTenant()
  const membership = memberships.find(
    (item) => item.tenant_id === organizationId && item.status === 'active',
  )
  const isAdmin = membership?.role === 'admin'
  const [sourceType, setSourceType] = useState<SourceType>('device')
  const [sourceId, setSourceId] = useState('')
  const [localRange, setLocalRange] = useState<LocalHistoryRange>(() =>
    defaultHistoryRange(),
  )
  const [appliedRange, setAppliedRange] = useState<ApiHistoryRange | null>(
    () => validateHistoryRange(defaultHistoryRange()).value ?? null,
  )
  const [rangeError, setRangeError] = useState<string | null>(null)
  const [positionCursors, setPositionCursors] = useState<(string | null)[]>([
    null,
  ])
  const [deviceCursor, setDeviceCursor] = useState<string | null>(null)
  const [sessionCursor, setSessionCursor] = useState<string | null>(null)
  const [auditCursors, setAuditCursors] = useState<(string | null)[]>([null])
  const [auditAction, setAuditAction] = useState('')
  const [auditAppliedAction, setAuditAppliedAction] = useState('')
  const [auditRange, setAuditRange] = useState<ApiHistoryRange | null>(
    () => validateHistoryRange(defaultHistoryRange()).value ?? null,
  )
  const timeZone = browserTimeZone()
  const currentCursor = positionCursors.at(-1) ?? null
  const auditCursor = auditCursors.at(-1) ?? null

  const devices = useQuery({
    queryKey: organizationQueryKey(
      organizationId ?? 'none',
      'history-devices',
      deviceCursor,
    ),
    enabled: !!organizationId && sourceType === 'device',
    queryFn: async ({ signal }) => {
      const { data, error } = await portalApi.GET('/devices', {
        params: {
          header: { 'X-Organization-Id': organizationId! },
          query: {
            limit: 100,
            ...(deviceCursor ? { cursor: deviceCursor } : {}),
          },
        },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar device tidak tersedia')
      return assertTenantPage(data as Page<Device>, organizationId!)
    },
  })
  const sessions = useQuery({
    queryKey: organizationQueryKey(
      organizationId ?? 'none',
      'history-sessions',
      sessionCursor,
    ),
    enabled: !!organizationId && sourceType === 'session',
    queryFn: async ({ signal }) => {
      const { data, error } = await portalApi.GET('/sessions', {
        params: {
          header: { 'X-Organization-Id': organizationId! },
          query: {
            limit: 100,
            ...(sessionCursor ? { cursor: sessionCursor } : {}),
          },
        },
        signal,
      })
      if (error || !data) throw error ?? new Error('Daftar sesi tidak tersedia')
      return assertTenantPage(data as Page<Session>, organizationId!)
    },
  })
  const options =
    sourceType === 'device'
      ? (devices.data?.data ?? [])
      : (sessions.data?.data ?? [])
  const sourcePage = sourceType === 'device' ? devices.data : sessions.data
  const sourceLoading =
    sourceType === 'device' ? devices.isLoading : sessions.isLoading
  const sourceError = sourceType === 'device' ? devices.error : sessions.error

  const positionPage = useQuery({
    queryKey: organizationQueryKey(
      organizationId ?? 'none',
      'history-positions',
      sourceType,
      sourceId,
      appliedRange?.from,
      appliedRange?.to,
      currentCursor,
    ),
    enabled: !!organizationId && !!sourceId && !!appliedRange,
    queryFn: async ({ signal }) => {
      const query = {
        from: appliedRange!.from,
        to: appliedRange!.to,
        limit: 100,
        ...(currentCursor ? { cursor: currentCursor } : {}),
      }
      const result =
        sourceType === 'device'
          ? await portalApi.GET('/devices/{device_id}/positions', {
              params: {
                header: { 'X-Organization-Id': organizationId! },
                path: { device_id: sourceId },
                query,
              },
              signal,
            })
          : await portalApi.GET('/sessions/{session_id}/positions', {
              params: {
                header: { 'X-Organization-Id': organizationId! },
                path: { session_id: sourceId },
                query,
              },
              signal,
            })
      if (result.error || !result.data)
        throw result.error ?? new Error('Histori posisi tidak tersedia')
      const page = result.data as components['schemas']['PositionPage']
      assertTenantPage(page, organizationId!)
      if (
        sourceType === 'device' &&
        page.data.some((row) => row.device_id !== sourceId)
      )
        throw new Error('Respons posisi tidak cocok dengan device terpilih.')
      return page
    },
  })
  const mapPages = useInfiniteQuery({
    queryKey: organizationQueryKey(
      organizationId ?? 'none',
      'history-map',
      sourceType,
      sourceId,
      appliedRange?.from,
      appliedRange?.to,
    ),
    enabled: !!organizationId && !!sourceId && !!appliedRange,
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam, signal }) => {
      const query = {
        from: appliedRange!.from,
        to: appliedRange!.to,
        limit: HISTORY_MAP_PAGE_SIZE,
        ...(pageParam ? { cursor: pageParam } : {}),
      }
      const result =
        sourceType === 'device'
          ? await portalApi.GET('/devices/{device_id}/positions', {
              params: {
                header: { 'X-Organization-Id': organizationId! },
                path: { device_id: sourceId },
                query,
              },
              signal,
            })
          : await portalApi.GET('/sessions/{session_id}/positions', {
              params: {
                header: { 'X-Organization-Id': organizationId! },
                path: { session_id: sourceId },
                query,
              },
              signal,
            })
      if (result.error || !result.data)
        throw result.error ?? new Error('Data peta tidak tersedia')
      const page = result.data as components['schemas']['PositionPage']
      assertTenantPage(page, organizationId!)
      if (
        sourceType === 'device' &&
        page.data.some((row) => row.device_id !== sourceId)
      )
        throw new Error('Respons posisi tidak cocok dengan device terpilih.')
      return page
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
  const positions = positionPage.data?.data ?? []
  const mapPositions = useMemo(
    () =>
      mapPages.data?.pages
        .flatMap((page) => page.data)
        .slice(0, HISTORY_MAP_POINT_LIMIT) ?? [],
    [mapPages.data],
  )
  const mapHasMore = !!mapPages.hasNextPage
  const mapAtLimit = mapPositions.length >= HISTORY_MAP_POINT_LIMIT
  const audit = useQuery({
    queryKey: organizationQueryKey(
      organizationId ?? 'none',
      'history-audit',
      auditRange?.from,
      auditRange?.to,
      auditAppliedAction,
      auditCursor,
    ),
    enabled: !!organizationId && isAdmin && !!auditRange,
    queryFn: async ({ signal }) => {
      const { data, error } = await portalApi.GET(
        '/organization/audit-events',
        {
          params: {
            header: { 'X-Organization-Id': organizationId! },
            query: {
              from: auditRange!.from,
              to: auditRange!.to,
              limit: 100,
              ...(auditAppliedAction ? { action: auditAppliedAction } : {}),
              ...(auditCursor ? { cursor: auditCursor } : {}),
            },
          },
          signal,
        },
      )
      if (error || !data)
        throw error ?? new Error('Audit organisasi tidak tersedia')
      return assertTenantPage(data as Page<AuditEvent>, organizationId!)
    },
  })

  function applyRange(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const parsed = validateHistoryRange(localRange)
    if (parsed.error) {
      setRangeError(parsed.error)
      return
    }
    setRangeError(null)
    setAppliedRange(parsed.value)
    setAuditRange(parsed.value)
    setPositionCursors([null])
    setAuditCursors([null])
  }
  function changeSourceType(value: SourceType) {
    setSourceType(value)
    setSourceId('')
    setPositionCursors([null])
  }
  function applyAuditFilter(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setAuditAppliedAction(auditAction.trim())
    setAuditCursors([null])
  }
  function goToNextPositionPage() {
    if (positionPage.data?.next_cursor)
      setPositionCursors((current) => [
        ...current,
        positionPage.data!.next_cursor,
      ])
  }
  function goToNextAuditPage() {
    if (audit.data?.next_cursor)
      setAuditCursors((current) => [...current, audit.data!.next_cursor])
  }

  return (
    <section className="space-y-6" aria-labelledby="history-title">
      <header>
        <p className="text-muted-foreground text-sm">Management</p>
        <h1
          className="text-3xl font-semibold tracking-tight"
          id="history-title"
        >
          Peta dan histori
        </h1>
        <p className="text-muted-foreground mt-2 text-sm">
          Riwayat lokasi device mengikuti grant akses dan organisasi aktif. Zona
          waktu tampilan: {timeZone}; API menerima waktu UTC.
        </p>
      </header>
      <HistoryFilterForm
        sourceType={sourceType}
        onSourceTypeChange={changeSourceType}
        sourceId={sourceId}
        onSourceIdChange={(id) => {
          setSourceId(id)
          setPositionCursors([null])
        }}
        options={options}
        localRange={localRange}
        setLocalRange={setLocalRange}
        onSubmit={applyRange}
        timeZone={timeZone}
        sourceLoading={sourceLoading}
        sourceError={sourceError}
        rangeError={rangeError}
        hasMoreSources={!!sourcePage?.next_cursor}
        onLoadMoreSources={() => {
          if (sourceType === 'device')
            setDeviceCursor(sourcePage?.next_cursor ?? null)
          else setSessionCursor(sourcePage?.next_cursor ?? null)
        }}
      />
      {sourceId && appliedRange && (
        <HistoryPositionPanel
          positions={positions}
          mapPositions={mapPositions}
          timeZone={timeZone}
          mapHasMore={mapHasMore}
          mapAtLimit={mapAtLimit}
          mapLoadingNext={mapPages.isFetchingNextPage}
          mapError={mapPages.error}
          onLoadMapPage={() => void mapPages.fetchNextPage()}
          tableLoading={positionPage.isLoading}
          tableError={positionPage.error}
          positionCursors={positionCursors}
          onPreviousPage={() =>
            setPositionCursors((current) => current.slice(0, -1))
          }
          onNextPage={goToNextPositionPage}
          hasNextPage={!!positionPage.data?.next_cursor}
          tableFetching={positionPage.isFetching}
        />
      )}
      {isAdmin ? (
        <HistoryAuditPanel
          action={auditAction}
          onActionChange={setAuditAction}
          onSubmit={applyAuditFilter}
          timeZone={timeZone}
          loading={audit.isLoading}
          error={audit.error}
          events={audit.data?.data ?? []}
          cursors={auditCursors}
          onPrevious={() => setAuditCursors((current) => current.slice(0, -1))}
          onNext={goToNextAuditPage}
          hasNextPage={!!audit.data?.next_cursor}
          fetching={audit.isFetching}
        />
      ) : (
        <p className="text-muted-foreground text-sm">
          Audit organisasi hanya tersedia bagi admin organisasi.
        </p>
      )}
      {!auth.me && <p role="status">Memeriksa sesi login…</p>}
    </section>
  )
}
