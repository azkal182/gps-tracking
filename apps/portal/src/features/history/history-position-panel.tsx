import type { ColumnDef } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import type { components } from '@/api/generated'
import { formatHistoryTime } from './history-utils'
import { HistoryMap } from './history-map'
import { HistoryTable } from './history-table'
import { historyErrorMessage } from './history-error'

type Position = components['schemas']['Position']

function column<T>(
  id: string,
  header: string,
  render: (row: T) => ReactNode,
): ColumnDef<T, unknown> {
  return { id, header, cell: ({ row }) => render(row.original) }
}

export function HistoryPositionPanel({
  positions,
  mapPositions,
  timeZone,
  mapHasMore,
  mapAtLimit,
  mapLoadingNext,
  mapError,
  onLoadMapPage,
  tableLoading,
  tableError,
  positionCursors,
  onPreviousPage,
  onNextPage,
  hasNextPage,
  tableFetching,
}: {
  positions: Position[]
  mapPositions: Position[]
  timeZone: string
  mapHasMore: boolean
  mapAtLimit: boolean
  mapLoadingNext: boolean
  mapError: unknown
  onLoadMapPage: () => void
  tableLoading: boolean
  tableError: unknown
  positionCursors: (string | null)[]
  onPreviousPage: () => void
  onNextPage: () => void
  hasNextPage: boolean
  tableFetching: boolean
}) {
  return (
    <>
      <section
        className="bg-background overflow-hidden rounded-xl border"
        aria-labelledby="history-map-title"
      >
        <div className="border-b px-4 py-3">
          <h2 className="font-semibold" id="history-map-title">
            Peta histori
          </h2>
          <p className="text-muted-foreground mt-1 text-sm">
            Peta memuat halaman berisi maksimal 1.000 posisi per permintaan.
            Tabel dapat dilanjutkan secara terpisah.
          </p>
        </div>
        <HistoryMap positions={mapPositions} />
        <div className="flex flex-wrap items-center justify-between gap-3 border-t p-4 text-sm">
          <p className="text-muted-foreground" role="status">
            {mapAtLimit && mapHasMore
              ? 'Peta hanya menampilkan sebagian data hingga 10.000 titik. Persempit rentang; tabel tetap dapat dilanjutkan.'
              : mapHasMore
                ? `Peta menampilkan ${mapPositions.length.toLocaleString('id-ID')} titik; halaman peta berikutnya belum dimuat.`
                : `Peta menampilkan ${mapPositions.length.toLocaleString('id-ID')} titik pada rentang terpilih.`}
          </p>
          <Button
            disabled={!mapHasMore || mapAtLimit || mapLoadingNext}
            onClick={onLoadMapPage}
            size="sm"
            type="button"
            variant="outline"
          >
            {mapLoadingNext
              ? 'Memuat…'
              : mapAtLimit
                ? 'Batas peta tercapai'
                : 'Tambahkan halaman ke peta'}
          </Button>
        </div>
        {mapError != null && (
          <p className="text-destructive px-4 pb-4 text-sm" role="alert">
            {historyErrorMessage(mapError)}
          </p>
        )}
      </section>
      <section
        className="bg-background overflow-hidden rounded-xl border"
        aria-labelledby="history-positions-title"
      >
        <div className="border-b px-4 py-3">
          <h2 className="font-semibold" id="history-positions-title">
            Tabel posisi
          </h2>
          <p className="text-muted-foreground mt-1 text-sm">
            Waktu server disimpan dalam UTC dan ditampilkan dengan zona waktu{' '}
            {timeZone}. Batas retensi adalah satu tahun.
          </p>
        </div>
        {tableLoading ? (
          <p className="p-4 text-sm" role="status">
            Memuat histori…
          </p>
        ) : tableError != null ? (
          <p className="text-destructive p-4 text-sm" role="alert">
            {historyErrorMessage(tableError)}
          </p>
        ) : (
          <HistoryTable
            columns={[
              column<Position>('recorded_at', 'Waktu dicatat', (row) =>
                formatHistoryTime(row.recorded_at, timeZone),
              ),
              column<Position>('latitude', 'Latitude', (row) => row.latitude),
              column<Position>(
                'longitude',
                'Longitude',
                (row) => row.longitude,
              ),
              column<Position>(
                'speed_mps',
                'Kecepatan (m/s)',
                (row) => row.speed_mps ?? '—',
              ),
              column<Position>(
                'accuracy_m',
                'Akurasi (m)',
                (row) => row.accuracy_m ?? '—',
              ),
            ]}
            data={positions}
            emptyMessage="Tidak ada posisi tersimpan dalam rentang ini."
            getRowId={(row) => row.id}
          />
        )}
        <div className="flex justify-end gap-2 border-t p-4">
          <Button
            disabled={positionCursors.length < 2 || tableFetching}
            onClick={onPreviousPage}
            size="sm"
            type="button"
            variant="outline"
          >
            Sebelumnya
          </Button>
          <Button
            disabled={!hasNextPage || tableFetching}
            onClick={onNextPage}
            size="sm"
            type="button"
            variant="outline"
          >
            Berikutnya
          </Button>
        </div>
      </section>
    </>
  )
}
