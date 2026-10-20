import { usePortalTenant } from '@/portal-context'
import { Button } from '@/components/ui/button'
import { portalErrorMessage } from '@/api/client'
import { LiveMap } from './live-map'
import { useLiveTargets } from './use-live-targets'

const jakartaTime = new Intl.DateTimeFormat('id-ID', {
  year: 'numeric',
  month: 'short',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  timeZone: 'Asia/Jakarta',
  timeZoneName: 'short',
})

function timestamp(value: string | null) {
  return value ? jakartaTime.format(new Date(value)) : 'Belum ada kontak'
}

function connectivityLabel(value: string | null) {
  if (value === 'online') return 'Online'
  if (value === 'offline') return 'Offline'
  if (value === 'unknown') return 'Belum diketahui'
  return '—'
}

function positionLabel(value: string) {
  if (value === 'fresh') return 'Posisi terbaru'
  if (value === 'stale') return 'Posisi lama'
  return 'Belum ada posisi'
}

export function LiveMapPage() {
  const { organizationId } = usePortalTenant()
  return (
    <LiveMapContent
      key={organizationId ?? 'no-organization'}
      organizationId={organizationId}
    />
  )
}

function LiveMapContent({ organizationId }: { organizationId: string | null }) {
  const live = useLiveTargets(organizationId)
  const active = live.rows.filter(
    (row) => row.location.tracking_status === 'active',
  )
  const positioned = active.filter((row) => row.location.position !== null)

  return (
    <section className="space-y-5" aria-labelledby="live-map-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1
            className="text-2xl font-semibold tracking-tight"
            id="live-map-title"
          >
            Peta live
          </h1>
          <p className="text-muted-foreground mt-1 text-sm">
            Posisi hanya berasal dari sesi tracking target yang sedang aktif.
          </p>
        </div>
        <div className="flex items-center gap-3">
          <span
            className="text-muted-foreground text-sm"
            role="status"
            aria-live="polite"
          >
            {live.connection === 'connected'
              ? 'Realtime tersambung'
              : live.connection === 'reconnecting'
                ? 'Menghubungkan ulang…'
                : live.connection === 'connecting'
                  ? 'Menghubungkan…'
                  : 'Realtime belum tersedia'}
          </span>
          <Button onClick={live.refresh} size="sm" variant="outline">
            Muat ulang
          </Button>
        </div>
      </div>

      {live.socketError && (
        <p className="text-muted-foreground text-sm" role="status">
          {live.socketError}
        </p>
      )}
      {live.error && (
        <p className="text-destructive text-sm" role="alert">
          {portalErrorMessage(live.error)}
        </p>
      )}
      {live.restrictedCount > 0 && (
        <p className="text-muted-foreground text-sm" role="status">
          Lokasi untuk {live.restrictedCount} target tidak ditampilkan karena
          akses posisi tidak diberikan.
        </p>
      )}

      <div className="grid gap-3 sm:grid-cols-3">
        <div className="bg-background rounded-xl border p-4">
          <p className="text-muted-foreground text-sm">Target</p>
          <p className="mt-1 text-xl font-semibold">{live.rows.length}</p>
        </div>
        <div className="bg-background rounded-xl border p-4">
          <p className="text-muted-foreground text-sm">Sesi aktif</p>
          <p className="mt-1 text-xl font-semibold">{active.length}</p>
        </div>
        <div className="bg-background rounded-xl border p-4">
          <p className="text-muted-foreground text-sm">Memiliki posisi</p>
          <p className="mt-1 text-xl font-semibold">{positioned.length}</p>
        </div>
      </div>

      {live.isLoading ? (
        <div
          className="bg-muted/30 text-muted-foreground grid min-h-[24rem] place-items-center rounded-xl border"
          role="status"
        >
          Memuat status target…
        </div>
      ) : (
        <LiveMap rows={live.rows} />
      )}

      <div className="bg-background overflow-hidden rounded-xl border">
        <div className="border-b px-4 py-3">
          <h2 className="font-semibold">Status target</h2>
          <p className="text-muted-foreground mt-1 text-sm">
            Konektivitas, umur posisi, dan sesi ditampilkan terpisah. Heartbeat
            memperbarui kontak tanpa mengubah marker.
          </p>
        </div>
        {live.rows.length === 0 && !live.isLoading ? (
          <p className="text-muted-foreground p-5 text-sm">
            Belum ada target yang dapat ditampilkan pada organisasi ini.
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[760px] text-left text-sm">
              <thead className="bg-muted/40 text-muted-foreground">
                <tr>
                  <th className="px-4 py-3 font-medium">Target</th>
                  <th className="px-4 py-3 font-medium">Sesi tracking</th>
                  <th className="px-4 py-3 font-medium">Konektivitas</th>
                  <th className="px-4 py-3 font-medium">Posisi</th>
                  <th className="px-4 py-3 font-medium">
                    Kontak terakhir (WIB)
                  </th>
                  <th className="px-4 py-3 font-medium">
                    Sampel dicatat (WIB)
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {live.rows.map(({ target, location }) => (
                  <tr key={target.id}>
                    <td className="px-4 py-3 font-medium">{target.name}</td>
                    <td className="px-4 py-3">
                      {location.tracking_status === 'active'
                        ? 'Aktif'
                        : 'Tidak aktif'}
                    </td>
                    <td className="px-4 py-3">
                      {connectivityLabel(location.connectivity_status)}
                    </td>
                    <td className="px-4 py-3">
                      {positionLabel(location.position_status)}
                    </td>
                    <td className="px-4 py-3">
                      {timestamp(location.last_contact_at)}
                    </td>
                    <td className="px-4 py-3">
                      {timestamp(location.position?.recorded_at ?? null)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  )
}
