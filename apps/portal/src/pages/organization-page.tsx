import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { portalApi, portalErrorMessage, withPortalMutation } from '@/api/client'
import {
  usePortalTenant,
  organizationQueryKey,
  useOrganizationQuery,
} from '@/portal-context'
import { Button } from '@/components/ui/button'
import { MemberAccessPanel } from '@/features/organization/member-access-panel'

const invitationSchema = z.object({
  email: z.email(),
  role: z.enum(['admin', 'operator', 'viewer']),
})
const nameSchema = z.object({ name: z.string().trim().min(1).max(120) })
const reauthenticationSchema = z.object({
  password: z.string().min(15).max(128),
  code: z.string().optional(),
})

function RecentAuthConfirmation({
  onConfirm,
  onCancel,
}: {
  onConfirm: (
    credentials: z.infer<typeof reauthenticationSchema>,
  ) => Promise<void>
  onCancel: () => void
}) {
  const form = useForm<z.infer<typeof reauthenticationSchema>>({
    resolver: zodResolver(reauthenticationSchema),
  })
  const [error, setError] = useState<string | null>(null)
  return (
    <form
      aria-label="Konfirmasi autentikasi terbaru"
      className="bg-background grid gap-3 rounded-xl border p-5 sm:grid-cols-[1fr_12rem_auto_auto]"
      onSubmit={form.handleSubmit(async (credentials) => {
        setError(null)
        try {
          await onConfirm(credentials)
        } catch (cause) {
          setError(portalErrorMessage(cause))
        } finally {
          form.reset()
        }
      })}
    >
      <label className="space-y-1 text-sm">
        <span>Password saat ini</span>
        <input
          className="bg-background h-9 w-full rounded-md border px-3"
          type="password"
          autoComplete="current-password"
          {...form.register('password')}
        />
      </label>
      <label className="space-y-1 text-sm">
        <span>Kode MFA jika diminta</span>
        <input
          className="bg-background h-9 w-full rounded-md border px-3"
          autoComplete="one-time-code"
          inputMode="numeric"
          {...form.register('code')}
        />
      </label>
      <Button
        className="self-end"
        disabled={form.formState.isSubmitting}
        size="sm"
        type="submit"
      >
        Konfirmasi
      </Button>
      <Button
        className="self-end"
        onClick={onCancel}
        size="sm"
        type="button"
        variant="outline"
      >
        Batal
      </Button>
      {error && (
        <p className="text-destructive text-sm sm:col-span-4" role="alert">
          {error}
        </p>
      )}
    </form>
  )
}

export function OrganizationPage() {
  const { organizationId, memberships, selectOrganization } = usePortalTenant()
  const queryClient = useQueryClient()
  const [notice, setNotice] = useState<string | null>(null)
  const [pendingAction, setPendingAction] = useState<{
    tenantId: string | null
    run: () => Promise<unknown>
  } | null>(null)
  const requestRecentAuthentication = (run: () => Promise<unknown>) => {
    setNotice(null)
    setPendingAction({ tenantId: organizationId, run })
  }
  const [invitationCursorState, setInvitationCursorState] = useState<{
    tenantId: string | null
    cursors: (string | null)[]
  }>({ tenantId: null, cursors: [null] })
  const invitationCursors =
    invitationCursorState.tenantId === organizationId
      ? invitationCursorState.cursors
      : [null]
  const invitationCursor = invitationCursors.at(-1) ?? null
  const setInvitationCursors = (
    update: (current: (string | null)[]) => (string | null)[],
  ) =>
    setInvitationCursorState((current) => ({
      tenantId: organizationId,
      cursors: update(
        current.tenantId === organizationId ? current.cursors : [null],
      ),
    }))
  const orgList = useQuery({
    queryKey: ['my-organizations'],
    queryFn: async ({ signal }) => {
      const { data, error } = await portalApi.GET('/organizations', {
        params: { query: { limit: 100 } },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar organisasi tidak tersedia')
      return data.data
    },
  })
  const organization = useOrganizationQuery(
    organizationId,
    'detail',
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/organization', {
        params: { header: { 'X-Organization-Id': tenantId } },
        signal,
      })
      if (error || !data) throw error ?? new Error('Organisasi tidak tersedia')
      if (data.id !== tenantId || data.tenant_id !== tenantId)
        throw new Error('Respons organisasi tidak cocok dengan tab aktif.')
      return data
    },
  )
  const activeMembership = memberships.find(
    (m) => m.tenant_id === organizationId && m.status === 'active',
  )
  const canManage = activeMembership?.role === 'admin'
  const invitations = useOrganizationQuery(
    organizationId,
    `invitations:${invitationCursor ?? 'first'}`,
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/organization/invitations', {
        params: {
          header: { 'X-Organization-Id': tenantId },
          query: {
            limit: 100,
            ...(invitationCursor ? { cursor: invitationCursor } : {}),
          },
        },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar undangan tidak tersedia')
      return data
    },
    !!canManage,
  )
  const refreshOrg = async () => {
    if (!organizationId) return
    await queryClient.invalidateQueries({
      queryKey: organizationQueryKey(organizationId),
    })
    await queryClient.invalidateQueries({ queryKey: ['my-organizations'] })
  }
  const confirmRecentAuthentication = async (
    credentials: z.infer<typeof reauthenticationSchema>,
  ) => {
    await withPortalMutation(async () => {
      const { error } = await portalApi.POST('/auth/reauthenticate', {
        body: {
          password: credentials.password,
          ...(credentials.code ? { code: credentials.code } : {}),
        },
      })
      if (error) throw error
    })
    if (!pendingAction || pendingAction.tenantId !== organizationId)
      throw new Error('Organisasi berubah. Silakan mulai ulang tindakan ini.')
    await pendingAction.run()
    setPendingAction(null)
  }
  const inviteForm = useForm<z.infer<typeof invitationSchema>>({
    resolver: zodResolver(invitationSchema),
    defaultValues: { role: 'operator' },
  })
  const inviteMutation = useMutation({
    mutationFn: async (values: z.infer<typeof invitationSchema>) =>
      withPortalMutation(async () => {
        const { data, error } = await portalApi.POST(
          '/organization/invitations',
          {
            params: { header: { 'X-Organization-Id': organizationId ?? '' } },
            body: values,
          },
        )
        if (error || !data)
          throw error ?? new Error('Undangan tidak dapat dibuat')
        return data
      }),
    onSuccess: async () => {
      inviteForm.reset({ role: 'operator', email: '' })
      setNotice('Undangan dibuat dan dimasukkan ke antrean email.')
      setInvitationCursors(() => [null])
      await refreshOrg()
    },
  })
  const nameForm = useForm<z.infer<typeof nameSchema>>({
    resolver: zodResolver(nameSchema),
  })
  const renameMutation = useMutation({
    mutationFn: async (values: z.infer<typeof nameSchema>) =>
      withPortalMutation(async () => {
        const { data, error } = await portalApi.PATCH('/organization', {
          params: { header: { 'X-Organization-Id': organizationId ?? '' } },
          body: values,
        })
        if (error || !data)
          throw error ?? new Error('Nama organisasi tidak dapat diperbarui')
        return data
      }),
    onSuccess: async () => {
      setNotice('Nama organisasi diperbarui.')
      await refreshOrg()
    },
  })
  const inviteActions = useMemo(
    () => ({
      resend: async (id: string) =>
        withPortalMutation(async () => {
          const { error } = await portalApi.POST(
            '/organization/invitations/{id}/resend',
            {
              params: {
                header: { 'X-Organization-Id': organizationId ?? '' },
                path: { id },
              },
              body: {},
            },
          )
          if (error) throw error
        }),
      revoke: async (id: string) =>
        withPortalMutation(async () => {
          const { error } = await portalApi.DELETE(
            '/organization/invitations/{id}',
            {
              params: {
                header: { 'X-Organization-Id': organizationId ?? '' },
                path: { id },
              },
            },
          )
          if (error) throw error
        }),
    }),
    [organizationId],
  )

  if (!organizationId)
    return (
      <section className="space-y-3">
        <h1 className="text-2xl font-semibold">Organisasi</h1>
        <p className="text-muted-foreground">
          Akun ini belum memiliki organisasi aktif.
        </p>
      </section>
    )
  return (
    <section className="mx-auto max-w-5xl space-y-8">
      <header className="space-y-2">
        <p className="text-muted-foreground text-sm">Workspace</p>
        <h1 className="text-3xl font-semibold">Organisasi</h1>
        <p className="text-muted-foreground text-sm">
          Konteks ini berlaku hanya pada tab browser ini.
        </p>
      </header>
      {orgList.isLoading || organization.isLoading ? (
        <p role="status">Memuat organisasi…</p>
      ) : null}
      {(orgList.error || organization.error) && (
        <p role="alert" className="text-destructive text-sm">
          {portalErrorMessage(orgList.error ?? organization.error)}
        </p>
      )}
      {orgList.data && orgList.data.length > 1 && (
        <label className="block max-w-lg space-y-2 text-sm font-medium">
          Organisasi aktif
          <select
            aria-label="Organisasi aktif"
            className="bg-background h-10 w-full rounded-md border px-3"
            value={organizationId}
            onChange={(event) => {
              void selectOrganization(event.target.value).catch((error) =>
                setNotice(portalErrorMessage(error)),
              )
            }}
          >
            {orgList.data.map((org) => (
              <option key={org.id} value={org.id}>
                {org.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {notice && (
        <p role="status" className="text-sm">
          {notice}
        </p>
      )}
      {pendingAction?.tenantId === organizationId && (
        <RecentAuthConfirmation
          onCancel={() => setPendingAction(null)}
          onConfirm={confirmRecentAuthentication}
        />
      )}
      {organization.data && (
        <article className="bg-background space-y-3 rounded-xl border p-5">
          <div>
            <h2 className="text-xl font-semibold">{organization.data.name}</h2>
            <p className="text-muted-foreground text-sm">
              {organization.data.status} · peran Anda:{' '}
              {activeMembership?.role ?? 'tidak aktif'}
            </p>
          </div>
          {canManage && (
            <form
              className="flex flex-wrap gap-2"
              onSubmit={nameForm.handleSubmit((values) =>
                requestRecentAuthentication(() =>
                  renameMutation.mutateAsync(values),
                ),
              )}
            >
              <input
                aria-label="Nama organisasi baru"
                className="bg-background h-9 min-w-60 rounded-md border px-3 text-sm"
                defaultValue={organization.data.name}
                {...nameForm.register('name')}
              />
              <Button
                disabled={renameMutation.isPending}
                size="sm"
                type="submit"
                variant="outline"
              >
                Simpan nama
              </Button>
            </form>
          )}
        </article>
      )}

      {canManage ? (
        <>
          <MemberAccessPanel
            key={organizationId}
            organizationId={organizationId}
            isAdmin={canManage}
          />
          <section className="bg-background space-y-4 rounded-xl border p-5">
            <header>
              <h2 className="text-xl font-semibold">Undang anggota</h2>
              <p className="text-muted-foreground text-sm">
                Email undangan berlaku 72 jam. Portal tidak menampilkan token
                undangan.
              </p>
            </header>
            <form
              className="grid gap-3 sm:grid-cols-[1fr_12rem_auto]"
              onSubmit={inviteForm.handleSubmit((values) =>
                requestRecentAuthentication(() =>
                  inviteMutation.mutateAsync(values),
                ),
              )}
            >
              <input
                aria-label="Email anggota"
                className="bg-background h-9 rounded-md border px-3 text-sm"
                placeholder="nama@organisasi.id"
                type="email"
                {...inviteForm.register('email')}
              />
              <select
                aria-label="Peran undangan"
                className="bg-background h-9 rounded-md border px-3 text-sm"
                {...inviteForm.register('role')}
              >
                <option value="operator">Operator</option>
                <option value="viewer">Viewer</option>
                <option value="admin">Admin (MFA wajib)</option>
              </select>
              <Button
                size="sm"
                type="submit"
                disabled={inviteMutation.isPending}
              >
                Kirim undangan
              </Button>
            </form>
            {(inviteForm.formState.errors.email ||
              inviteMutation.error ||
              renameMutation.error) && (
              <p role="alert" className="text-destructive text-sm">
                {inviteForm.formState.errors.email?.message ??
                  portalErrorMessage(
                    inviteMutation.error ?? renameMutation.error,
                  )}
              </p>
            )}
            <h3 className="font-medium">Undangan</h3>
            {invitations.isLoading && <p role="status">Memuat undangan…</p>}
            {invitations.error && (
              <p role="alert" className="text-destructive text-sm">
                {portalErrorMessage(invitations.error)}
              </p>
            )}
            <div className="divide-y">
              {invitations.data?.data.map((invite) => (
                <div
                  className="flex flex-wrap items-center justify-between gap-3 py-3 text-sm"
                  key={invite.id}
                >
                  <span>
                    {invite.email} · {invite.role}
                  </span>
                  <span className="text-muted-foreground">
                    {invite.status} · berlaku sampai{' '}
                    {new Date(invite.expires_at).toLocaleString()}
                  </span>
                  {invite.status === 'pending' && (
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() =>
                          requestRecentAuthentication(async () => {
                            await inviteActions.resend(invite.id)
                            await refreshOrg()
                          })
                        }
                      >
                        Kirim ulang
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                          requestRecentAuthentication(async () => {
                            await inviteActions.revoke(invite.id)
                            await refreshOrg()
                          })
                        }
                      >
                        Cabut
                      </Button>
                    </div>
                  )}
                </div>
              ))}
            </div>
            <div className="flex justify-end gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={invitationCursors.length < 2}
                onClick={() =>
                  setInvitationCursors((current) => current.slice(0, -1))
                }
              >
                Sebelumnya
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={!invitations.data?.next_cursor}
                onClick={() => {
                  if (invitations.data?.next_cursor)
                    setInvitationCursors((current) => [
                      ...current,
                      invitations.data!.next_cursor,
                    ])
                }}
              >
                Berikutnya
              </Button>
            </div>
          </section>
        </>
      ) : (
        <p className="text-muted-foreground text-sm">
          Manajemen anggota dan undangan hanya tersedia untuk admin organisasi.
        </p>
      )}
    </section>
  )
}
