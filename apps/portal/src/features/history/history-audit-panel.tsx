import type { FormEvent } from 'react'
import type { ColumnDef } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import type { components } from '@/api/generated'
import { formatHistoryTime } from './history-utils'
import { HistoryTable } from './history-table'
import { historyErrorMessage } from './history-error'

type AuditEvent = components['schemas']['AuditEvent']
function column<T>(
  id: string,
  header: string,
  render: (row: T) => ReactNode,
): ColumnDef<T, unknown> {
  return { id, header, cell: ({ row }) => render(row.original) }
}

export function HistoryAuditPanel({
  action,
  onActionChange,
  onSubmit,
  timeZone,
  loading,
  error,
  events,
  cursors,
  onPrevious,
  onNext,
  hasNextPage,
  fetching,
}: {
  action: string
  onActionChange: (value: string) => void
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  timeZone: string
  loading: boolean
  error: unknown
  events: AuditEvent[]
  cursors: (string | null)[]
  onPrevious: () => void
  onNext: () => void
  hasNextPage: boolean
  fetching: boolean
}) {
  return (
    <section
      className="bg-background overflow-hidden rounded-xl border"
      aria-labelledby="audit-title"
    >
      <div className="border-b px-4 py-3">
        <h2 className="font-semibold" id="audit-title">
          Audit organisasi
        </h2>
        <p className="text-muted-foreground mt-1 text-sm">
          Admin dapat meninjau metadata akses histori dan aktivitas management.
          Audit tidak menampilkan koordinat maupun credential.
        </p>
      </div>
      <form
        className="flex flex-wrap items-end gap-3 border-b p-4"
        onSubmit={onSubmit}
      >
        <label className="grid min-w-60 flex-1 gap-1 text-sm">
          <span>Filter action (opsional)</span>
          <input
            className="bg-background h-9 rounded-md border px-3"
            value={action}
            onChange={(event) => onActionChange(event.target.value)}
            placeholder="history.read"
          />
        </label>
        <p className="text-muted-foreground text-xs">
          Rentang audit mengikuti filter histori · batas 31 hari
        </p>
        <Button size="sm" type="submit" variant="outline">
          Terapkan filter audit
        </Button>
      </form>
      {loading ? (
        <p className="p-4 text-sm" role="status">
          Memuat audit…
        </p>
      ) : error ? (
        <p className="text-destructive p-4 text-sm" role="alert">
          {historyErrorMessage(error)}
        </p>
      ) : (
        <HistoryTable
          columns={[
            column<AuditEvent>('occurred_at', 'Waktu', (row) =>
              formatHistoryTime(row.occurred_at, timeZone),
            ),
            column<AuditEvent>(
              'actor_id',
              'Aktor',
              (row) => row.actor_id ?? 'Sistem',
            ),
            column<AuditEvent>('action', 'Action', (row) => row.action),
            column<AuditEvent>(
              'resource_type',
              'Jenis resource',
              (row) => row.resource_type ?? '—',
            ),
            column<AuditEvent>('outcome', 'Hasil', (row) => row.outcome),
            column<AuditEvent>(
              'request_id',
              'Request ID',
              (row) => row.request_id,
            ),
          ]}
          data={events}
          emptyMessage="Tidak ada event audit pada rentang ini."
          getRowId={(row) => row.id}
        />
      )}
      <div className="flex justify-end gap-2 border-t p-4">
        <Button
          disabled={cursors.length < 2 || fetching}
          onClick={onPrevious}
          size="sm"
          type="button"
          variant="outline"
        >
          Sebelumnya
        </Button>
        <Button
          disabled={!hasNextPage || fetching}
          onClick={onNext}
          size="sm"
          type="button"
          variant="outline"
        >
          Berikutnya
        </Button>
      </div>
    </section>
  )
}
