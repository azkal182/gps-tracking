import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { portalApi, portalErrorMessage, withPortalMutation } from '@/api/client'
import { Button } from '@/components/ui/button'
import { usePortalAuth } from '@/portal-context'

export function SessionsPage() {
  const auth = usePortalAuth()
  const queryClient = useQueryClient()
  const [cursors, setCursors] = useState<(string | null)[]>([null])
  const [revokingId, setRevokingId] = useState<string | null>(null)
  const [revokeError, setRevokeError] = useState<string | null>(null)
  const cursor = cursors.at(-1) ?? null
  const sessions = useQuery({
    queryKey: ['auth', auth.me?.user.id, 'sessions', cursor],
    enabled: !!auth.me,
    queryFn: async ({ signal }) => {
      const { data, error } = await portalApi.GET('/auth/sessions', {
        params: { query: { limit: 100, ...(cursor ? { cursor } : {}) } },
        signal,
      })
      if (error || !data) throw error ?? new Error('Sesi tidak tersedia')
      return data
    },
  })
  async function revoke(id: string) {
    await withPortalMutation(async () => {
      const { error } = await portalApi.DELETE('/auth/sessions/{id}', {
        params: { path: { id } },
      })
      if (error) throw error
    })
    await queryClient.invalidateQueries({
      queryKey: ['auth', auth.me?.user.id, 'sessions'],
    })
    if (sessions.data?.data.find((item) => item.id === id)?.current)
      await auth.logout()
  }
  async function handleRevoke(id: string) {
    setRevokeError(null)
    setRevokingId(id)
    try {
      await revoke(id)
    } catch (error) {
      setRevokeError(portalErrorMessage(error))
    } finally {
      setRevokingId(null)
    }
  }
  return (
    <section className="mx-auto max-w-4xl space-y-6">
      <header>
        <p className="text-muted-foreground text-sm">Akun</p>
        <h1 className="text-3xl font-semibold">Sesi login</h1>
        <p className="text-muted-foreground mt-2 text-sm">
          Cabut sesi perangkat yang tidak Anda kenali. Tracking session tetap
          terpisah dari sesi login.
        </p>
      </header>
      {sessions.isLoading && <p role="status">Memuat sesi…</p>}
      {sessions.error && (
        <p role="alert" className="text-destructive text-sm">
          {portalErrorMessage(sessions.error)}
        </p>
      )}
      {revokeError && (
        <p role="alert" className="text-destructive text-sm">
          {revokeError}
        </p>
      )}
      <div className="bg-background divide-y rounded-xl border">
        {sessions.data?.data.map((session) => (
          <article
            className="flex flex-wrap items-center justify-between gap-4 p-4"
            key={session.id}
          >
            <div>
              <h2 className="font-medium">
                {session.device_label}
                {session.current ? ' · sesi ini' : ''}
              </h2>
              <p className="text-muted-foreground text-sm">
                Terakhir aktif{' '}
                {session.last_used_at
                  ? new Date(session.last_used_at).toLocaleString()
                  : 'belum tercatat'}{' '}
                · berakhir {new Date(session.expires_at).toLocaleString()}
              </p>
            </div>
            <Button
              disabled={revokingId === session.id}
              size="sm"
              variant="outline"
              onClick={() => void handleRevoke(session.id)}
            >
              {revokingId === session.id
                ? 'Memproses…'
                : session.current
                  ? 'Keluar'
                  : 'Cabut sesi'}
            </Button>
          </article>
        ))}
      </div>
      <div className="flex justify-end gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={cursors.length < 2}
          onClick={() => setCursors((current) => current.slice(0, -1))}
        >
          Sebelumnya
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!sessions.data?.next_cursor}
          onClick={() => {
            if (sessions.data?.next_cursor)
              setCursors((current) => [...current, sessions.data!.next_cursor])
          }}
        >
          Berikutnya
        </Button>
      </div>
    </section>
  )
}
