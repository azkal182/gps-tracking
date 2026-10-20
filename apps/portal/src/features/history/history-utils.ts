export const MAX_HISTORY_RANGE_MS = 31 * 24 * 60 * 60 * 1000
export const HISTORY_MAP_POINT_LIMIT = 10_000
export const HISTORY_MAP_PAGE_SIZE = 1_000

export type LocalHistoryRange = { from: string; to: string }
export type ApiHistoryRange = { from: string; to: string }

function toLocalInputValue(date: Date) {
  const offset = date.getTimezoneOffset() * 60_000
  return new Date(date.getTime() - offset).toISOString().slice(0, 16)
}

export function defaultHistoryRange(now = new Date()): LocalHistoryRange {
  return {
    from: toLocalInputValue(new Date(now.getTime() - 24 * 60 * 60 * 1000)),
    to: toLocalInputValue(now),
  }
}

export function validateHistoryRange(range: LocalHistoryRange) {
  const from = new Date(range.from)
  const to = new Date(range.to)
  if (
    !range.from ||
    !range.to ||
    !Number.isFinite(from.getTime()) ||
    !Number.isFinite(to.getTime())
  )
    return { error: 'Isi waktu mulai dan selesai dengan benar.' as const }
  const duration = to.getTime() - from.getTime()
  if (duration <= 0)
    return { error: 'Waktu selesai harus setelah waktu mulai.' as const }
  if (duration > MAX_HISTORY_RANGE_MS)
    return {
      error:
        'Rentang histori maksimal 31 hari. Persempit rentang waktu.' as const,
    }
  return {
    value: {
      from: from.toISOString(),
      to: to.toISOString(),
    } satisfies ApiHistoryRange,
  }
}

export function browserTimeZone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
}

export function visibleMapPositions<T>(positions: T[]) {
  return positions.slice(0, HISTORY_MAP_POINT_LIMIT)
}

export function formatHistoryTime(
  value: string | null,
  timeZone = browserTimeZone(),
) {
  if (!value) return '—'
  return new Intl.DateTimeFormat('id-ID', {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    timeZone,
    timeZoneName: 'short',
  }).format(new Date(value))
}
