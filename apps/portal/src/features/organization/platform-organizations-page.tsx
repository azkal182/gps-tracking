import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { portalApi, portalErrorMessage, withPortalMutation } from '@/api/client'
import { ConfirmAction } from '@/components/confirm-action'
import { Button } from '@/components/ui/button'
import {
  RecentAuthConfirmation,
  type Reauthentication,
} from './recent-auth-confirmation'
import { usePortalAuth } from '@/portal-context'

const createOrganizationSchema = z.object({
  name: z.string().trim().min(1).max(150),
  admin_email: z.email(),
})

type PendingAction = {
  label: string
  run: () => Promise<void>
} | null

export function PlatformOrganizationsPage() {
  const auth = usePortalAuth()
  const queryClient = useQueryClient()
  const [cursorStack, setCursorStack] = useState<(string | null)[]>([null])
  const [notice, setNotice] = useState<string | null>(null)
  const [pendingAction, setPendingAction] = useState<PendingAction>(null)
  const [reissueOrganization, setReissueOrganization] = useState<{
    id: string
    name: string
  } | null>(null)
  const [reissueEmail, setReissueEmail] = useState('')
  const cursor = cursorStack.at(-1) ?? null
  const organizations = useQuery({
    queryKey: ['platform-organizations', cursor],
    enabled: auth.me?.platform_admin === true,
    queryFn: async ({ signal }) => {
      const { data, error } = await portalApi.GET('/platform/organizations', {
        params: { query: { limit: 50, ...(cursor ? { cursor } : {}) } },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar organisasi platform tidak tersedia')
      return data
    },
  })
  const form = useForm<z.infer<typeof createOrganizationSchema>>({
    resolver: zodResolver(createOrganizationSchema),
  })
  const createOrganization = useMutation({
    mutationFn: async (values: z.infer<typeof createOrganizationSchema>) =>
      withPortalMutation(async () => {
        const { data, error } = await portalApi.POST(
          '/platform/organizations',
          {
            body: values,
          },
        )
        if (error || !data)
          throw error ?? new Error('Organisasi tidak dapat dibuat')
        return data
      }),
    onSuccess: async () => {
      form.reset()
      setNotice(
        'Organisasi dibuat dan undangan admin awal masuk ke antrean email.',
      )
      setCursorStack([null])
      await queryClient.invalidateQueries({
        queryKey: ['platform-organizations'],
      })
    },
  })

  const requestRecentAuthentication = (
    label: string,
    run: () => Promise<void>,
  ) => {
    setNotice(null)
    setPendingAction({ label, run })
  }
  const confirmRecentAuthentication = async (credentials: Reauthentication) => {
    await withPortalMutation(async () => {
      const { error } = await portalApi.POST('/auth/reauthenticate', {
        body: {
          password: credentials.password,
          ...(credentials.code ? { code: credentials.code } : {}),
        },
      })
      if (error) throw error
    })
    if (!pendingAction) throw new Error('Tindakan sudah dibatalkan.')
    await pendingAction.run()
    setPendingAction(null)
  }

  if (!auth.me?.platform_admin) {
    return (
      <section className="mx-auto max-w-5xl space-y-3">
        <h1 className="text-3xl font-semibold">
          Manajemen organisasi platform
        </h1>
        <p role="alert" className="text-muted-foreground text-sm">
          Halaman ini hanya tersedia untuk pengelola platform. Server tetap
          memvalidasi izin untuk setiap operasi.
        </p>
      </section>
    )
  }

  return (
    <section className="mx-auto max-w-5xl space-y-7">
      <header className="space-y-2">
        <p className="text-muted-foreground text-sm">Administrasi platform</p>
        <h1 className="text-3xl font-semibold">Organisasi</h1>
        <p className="text-muted-foreground max-w-2xl text-sm">
          Kelola status dan undangan admin awal. Halaman ini hanya memuat
          metadata organisasi dan tidak menampilkan lokasi atau histori.
        </p>
      </header>
      {notice && (
        <p role="status" className="rounded-lg border p-3 text-sm">
          {notice}
        </p>
      )}
      {pendingAction && (
        <RecentAuthConfirmation
          actionLabel={pendingAction.label}
          onCancel={() => setPendingAction(null)}
          onConfirm={confirmRecentAuthentication}
        />
      )}
      {reissueOrganization && (
        <form
          className="bg-background grid gap-3 rounded-xl border p-4 sm:grid-cols-[1fr_auto_auto]"
          onSubmit={(event) => {
            event.preventDefault()
            const parsed = z.email().safeParse(reissueEmail.trim())
            if (!parsed.success) {
              setNotice('Masukkan email admin awal yang valid.')
              return
            }
            requestRecentAuthentication(
              `Kirim ulang undangan admin awal untuk ${reissueOrganization.name}`,
              async () => {
                await withPortalMutation(async () => {
                  const { error } = await portalApi.POST(
                    '/platform/organizations/{id}/admin-invitations',
                    {
                      params: { path: { id: reissueOrganization.id } },
                      body: { email: parsed.data },
                    },
                  )
                  if (error) throw error
                })
                setNotice('Undangan admin awal dimasukkan ke antrean email.')
                setReissueOrganization(null)
                setReissueEmail('')
                await queryClient.invalidateQueries({
                  queryKey: ['platform-organizations'],
                })
              },
            )
          }}
        >
          <label className="grid gap-1 text-sm">
            Email admin awal
            <input
              className="bg-background h-9 rounded-md border px-3"
              type="email"
              value={reissueEmail}
              onChange={(event) => setReissueEmail(event.target.value)}
            />
          </label>
          <Button className="self-end" size="sm">
            Kirim ulang undangan
          </Button>
          <Button
            className="self-end"
            size="sm"
            type="button"
            variant="outline"
            onClick={() => setReissueOrganization(null)}
          >
            Batal
          </Button>
          <p className="text-muted-foreground text-xs sm:col-span-3">
            Hanya berhasil bila organisasi belum memiliki admin aktif; server
            menolak reissue setelah onboarding.
          </p>
        </form>
      )}
      <section className="bg-background space-y-4 rounded-xl border p-5">
        <h2 className="text-xl font-semibold">Buat organisasi</h2>
        <p className="text-muted-foreground text-sm">
          Backend membuat undangan admin pertama secara atomik. Portal tidak
          menerima atau menetapkan password admin.
        </p>
        <form
          className="grid gap-3 sm:grid-cols-[1fr_1fr_auto]"
          onSubmit={form.handleSubmit((values) =>
            requestRecentAuthentication(
              'Konfirmasi pembuatan organisasi',
              async () => {
                await createOrganization.mutateAsync(values)
              },
            ),
          )}
        >
          <label className="grid gap-1 text-sm">
            Nama organisasi
            <input
              className="bg-background h-9 rounded-md border px-3"
              {...form.register('name')}
            />
          </label>
          <label className="grid gap-1 text-sm">
            Email admin awal
            <input
              className="bg-background h-9 rounded-md border px-3"
              type="email"
              {...form.register('admin_email')}
            />
          </label>
          <Button
            className="self-end"
            size="sm"
            disabled={createOrganization.isPending}
            type="submit"
          >
            Buat dan undang admin
          </Button>
        </form>
        {(form.formState.errors.name ||
          form.formState.errors.admin_email ||
          createOrganization.error) && (
          <p role="alert" className="text-destructive text-sm">
            {form.formState.errors.name?.message ??
              form.formState.errors.admin_email?.message ??
              portalErrorMessage(createOrganization.error)}
          </p>
        )}
      </section>

      <section className="space-y-3">
        <h2 className="text-xl font-semibold">Semua organisasi</h2>
        {organizations.isLoading && <p role="status">Memuat organisasi…</p>}
        {organizations.error && (
          <p role="alert" className="text-destructive text-sm">
            {portalErrorMessage(organizations.error)}
          </p>
        )}
        {organizations.data?.data.length === 0 && (
          <p className="text-muted-foreground rounded-xl border border-dashed p-5 text-sm">
            Belum ada organisasi.
          </p>
        )}
        {organizations.data?.data.map((organization) => (
          <article
            key={organization.id}
            className="bg-background flex flex-wrap items-center justify-between gap-3 rounded-xl border p-4"
          >
            <div>
              <h3 className="font-medium">{organization.name}</h3>
              <p className="text-muted-foreground mt-1 text-xs">
                {organization.status} · {organization.id}
              </p>
            </div>
            {organization.status === 'active' ? (
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    setReissueOrganization({
                      id: organization.id,
                      name: organization.name,
                    })
                    setReissueEmail('')
                  }}
                >
                  Undang ulang admin awal
                </Button>
                <ConfirmAction
                  label="Tangguhkan"
                  confirmation="Akses tenant dan ingestion akan dihentikan; data tetap tersimpan."
                  confirmLabel="Lanjutkan ke autentikasi"
                  onConfirm={() =>
                    requestRecentAuthentication(
                      `Tangguhkan ${organization.name}`,
                      async () => {
                        await setStatus(organization.id, 'suspended')
                      },
                    )
                  }
                />
              </div>
            ) : (
              <ConfirmAction
                label="Aktifkan kembali"
                confirmation="Akses organisasi akan dipulihkan."
                confirmLabel="Lanjutkan ke autentikasi"
                onConfirm={() =>
                  requestRecentAuthentication(
                    `Aktifkan ${organization.name}`,
                    async () => {
                      await setStatus(organization.id, 'active')
                    },
                  )
                }
              />
            )}
          </article>
        ))}
        <div className="flex justify-end gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={cursorStack.length < 2}
            onClick={() => setCursorStack((stack) => stack.slice(0, -1))}
          >
            Sebelumnya
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={!organizations.data?.next_cursor}
            onClick={() => {
              if (organizations.data?.next_cursor)
                setCursorStack((stack) => [
                  ...stack,
                  organizations.data!.next_cursor,
                ])
            }}
          >
            Berikutnya
          </Button>
        </div>
      </section>
    </section>
  )

  async function setStatus(id: string, status: 'active' | 'suspended') {
    await withPortalMutation(async () => {
      const { error } = await portalApi.PATCH('/platform/organizations/{id}', {
        params: { path: { id } },
        body: { status },
      })
      if (error) throw error
    })
    setNotice(`Status organisasi diperbarui menjadi ${status}.`)
    await queryClient.invalidateQueries({
      queryKey: ['platform-organizations'],
    })
  }
}
