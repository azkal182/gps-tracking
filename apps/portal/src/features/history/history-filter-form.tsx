import type { FormEvent } from 'react'
import type { components } from '@/api/generated'
import { Button } from '@/components/ui/button'
import { formatHistoryTime, type LocalHistoryRange } from './history-utils'
import { historyErrorMessage } from './history-error'

type Device = components['schemas']['Device']
type Session = components['schemas']['TrackingSession']
type SourceType = 'device' | 'session'

export function HistoryFilterForm({
  sourceType,
  onSourceTypeChange,
  sourceId,
  onSourceIdChange,
  options,
  localRange,
  setLocalRange,
  onSubmit,
  timeZone,
  sourceLoading,
  sourceError,
  rangeError,
  hasMoreSources,
  onLoadMoreSources,
}: {
  sourceType: SourceType
  onSourceTypeChange: (value: SourceType) => void
  sourceId: string
  onSourceIdChange: (value: string) => void
  options: Device[] | Session[]
  localRange: LocalHistoryRange
  setLocalRange: (value: LocalHistoryRange) => void
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  timeZone: string
  sourceLoading: boolean
  sourceError: unknown
  rangeError: string | null
  hasMoreSources: boolean
  onLoadMoreSources: () => void
}) {
  return (
    <form
      className="bg-background grid gap-3 rounded-xl border p-4 sm:grid-cols-2 lg:grid-cols-5"
      onSubmit={onSubmit}
    >
      <label className="grid gap-1 text-sm">
        <span>Sumber histori</span>
        <select
          className="bg-background h-9 rounded-md border px-3"
          value={sourceType}
          onChange={(event) =>
            onSourceTypeChange(event.target.value as SourceType)
          }
        >
          <option value="device">Device</option>
          <option value="session">Sesi tracking</option>
        </select>
      </label>
      <label className="grid gap-1 text-sm">
        <span>{sourceType === 'device' ? 'Device' : 'Sesi tracking'}</span>
        <select
          className="bg-background h-9 min-w-0 rounded-md border px-3"
          value={sourceId}
          onChange={(event) => onSourceIdChange(event.target.value)}
        >
          <option value="">
            Pilih {sourceType === 'device' ? 'device' : 'sesi'}
          </option>
          {options.map((item) => (
            <option key={item.id} value={item.id}>
              {sourceType === 'device'
                ? `${(item as Device).name}${(item as Device).enabled ? '' : ' · nonaktif'}`
                : `${formatHistoryTime((item as Session).started_at, timeZone)} · ${(item as Session).status}`}
            </option>
          ))}
        </select>
      </label>
      <label className="grid gap-1 text-sm">
        <span>Dari (inklusif)</span>
        <input
          className="bg-background h-9 rounded-md border px-3"
          type="datetime-local"
          value={localRange.from}
          onChange={(event) =>
            setLocalRange({ ...localRange, from: event.target.value })
          }
        />
      </label>
      <label className="grid gap-1 text-sm">
        <span>Sampai (eksklusif)</span>
        <input
          className="bg-background h-9 rounded-md border px-3"
          type="datetime-local"
          value={localRange.to}
          onChange={(event) =>
            setLocalRange({ ...localRange, to: event.target.value })
          }
        />
      </label>
      <div className="flex items-end">
        <Button className="w-full" type="submit">
          Terapkan rentang
        </Button>
      </div>
      {sourceLoading && (
        <p
          className="text-muted-foreground text-sm sm:col-span-2 lg:col-span-5"
          role="status"
        >
          Memuat pilihan {sourceType}…
        </p>
      )}
      {sourceError != null && (
        <p
          className="text-destructive text-sm sm:col-span-2 lg:col-span-5"
          role="alert"
        >
          {historyErrorMessage(sourceError)}
        </p>
      )}
      {rangeError && (
        <p
          className="text-destructive text-sm sm:col-span-2 lg:col-span-5"
          role="alert"
        >
          {rangeError}
        </p>
      )}
      {hasMoreSources && (
        <div className="flex items-center gap-2 text-sm sm:col-span-2 lg:col-span-5">
          <span className="text-muted-foreground">
            Menampilkan pilihan halaman ini (maksimal 100).
          </span>
          <Button
            size="sm"
            type="button"
            variant="outline"
            onClick={onLoadMoreSources}
          >
            Muat pilihan berikutnya
          </Button>
        </div>
      )}
      {sourceType === 'device' &&
        options.some((item) => !(item as Device).enabled) && (
          <p className="text-muted-foreground text-xs sm:col-span-2 lg:col-span-5">
            Device nonaktif tetap dapat dipilih; posisinya masih tersedia selama
            masa retensi dan Anda memiliki akses.
          </p>
        )}
    </form>
  )
}
