import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { KeyRound, Plus, Radio, ShieldAlert } from 'lucide-react'
import { portalApi, portalErrorMessage, withPortalMutation } from '@/api/client'
import { Button } from '@/components/ui/button'
import {
  organizationQueryKey,
  useOrganizationQuery,
  usePortalAuth,
  usePortalTenant,
} from '@/portal-context'

type Target = { id: string; name: string; external_ref: string | null }
type Device = {
  id: string
  name: string
  identifier: string
  protocol: string
  enabled: boolean
  ingestion_policy: 'session_only' | 'always'
}
type TrackingSession = {
  id: string
  target_id: string
  device_id: string
  status: string
  started_at: string
  ended_at: string | null
}

const targetSchema = z.object({
  name: z.string().trim().min(1).max(150),
  external_ref: z.string().trim().max(150).optional(),
})
const deviceSchema = z.object({
  name: z.string().trim().min(1).max(150),
  identifier: z.string().trim().min(1).max(200),
  protocol: z.literal('http-json'),
  ingestion_policy: z.enum(['session_only', 'always']),
})
const sessionSchema = z.object({ target_id: z.uuid(), device_id: z.uuid() })
const reauthenticationSchema = z.object({
  password: z.string().min(15).max(128),
  code: z.string().optional(),
})

function FormError({ error }: { error: unknown }) {
  return (
    <p className="text-destructive text-sm" role="alert">
      {portalErrorMessage(error)}
    </p>
  )
}

function InlineForm({
  title,
  children,
  onSubmit,
  onCancel,
  busy,
}: {
  title: string
  children: React.ReactNode
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void
  onCancel: () => void
  busy: boolean
}) {
  return (
    <form
      className="bg-background grid gap-3 rounded-xl border p-4 sm:grid-cols-2"
      onSubmit={onSubmit}
    >
      <h3 className="font-medium sm:col-span-2">{title}</h3>
      {children}
      <div className="flex gap-2 sm:col-span-2">
        <Button disabled={busy} size="sm" type="submit">
          Simpan
        </Button>
        <Button
          disabled={busy}
          size="sm"
          type="button"
          variant="outline"
          onClick={onCancel}
        >
          Batal
        </Button>
      </div>
    </form>
  )
}

export function TrackingManagementPage() {
  const { organizationId } = usePortalTenant()
  return (
    <TrackingManagementContent
      key={organizationId ?? 'no-organization'}
      organizationId={organizationId}
    />
  )
}

function TrackingManagementContent({
  organizationId,
}: {
  organizationId: string | null
}) {
  const { me } = usePortalAuth()
  const queryClient = useQueryClient()
  const [form, setForm] = useState<'target' | 'device' | 'session' | null>(null)
  const [targetToEdit, setTargetToEdit] = useState<Target | null>(null)
  const [oneTimeToken, setOneTimeToken] = useState<{
    device: string
    token: string
  } | null>(null)
  const [credentialDevice, setCredentialDevice] = useState<Device | null>(null)
  const [rotatingCredential, setRotatingCredential] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const membership = me?.memberships.find(
    (item) => item.tenant_id === organizationId && item.status === 'active',
  )
  const isAdmin = membership?.role === 'admin'
  const effectiveGrants = useOrganizationQuery(
    organizationId,
    `self-grants:${me?.user.id ?? 'none'}`,
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET(
        '/organization/members/{user_id}/grants',
        {
          params: {
            header: { 'X-Organization-Id': tenantId },
            path: { user_id: me!.user.id },
          },
          signal,
        },
      )
      if (error || !data)
        throw error ?? new Error('Izin organisasi tidak tersedia')
      return data.grants
    },
    !!me && !isAdmin,
  )
  const hasGrant = (
    type: 'organization' | 'target' | 'device',
    id: string,
    permission:
      | 'targets:create'
      | 'devices:create'
      | 'read'
      | 'write'
      | 'credentials:write',
  ) =>
    !!isAdmin ||
    !!effectiveGrants.data?.some(
      (grant) =>
        grant.resource_type === type &&
        grant.resource_id === id &&
        grant.permissions.some((granted) => granted === permission),
    )
  const canCreateTarget =
    !!organizationId &&
    hasGrant('organization', organizationId, 'targets:create')
  const canCreateDevice =
    !!organizationId &&
    hasGrant('organization', organizationId, 'devices:create')
  const canWriteTarget = (id: string) => hasGrant('target', id, 'write')
  const canWriteDevice = (id: string) => hasGrant('device', id, 'write')
  const canRotateDevice = (id: string) =>
    hasGrant('device', id, 'credentials:write')
  const targets = useOrganizationQuery(
    organizationId,
    'tracking-targets',
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/targets', {
        params: {
          header: { 'X-Organization-Id': tenantId },
          query: { limit: 100 },
        },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar target tidak tersedia')
      return data.data as Target[]
    },
  )
  const devices = useOrganizationQuery(
    organizationId,
    'tracking-devices',
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/devices', {
        params: {
          header: { 'X-Organization-Id': tenantId },
          query: { limit: 100 },
        },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar device tidak tersedia')
      return data.data as Device[]
    },
  )
  const trackingSessions = useOrganizationQuery(
    organizationId,
    'tracking-sessions',
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/sessions', {
        params: {
          header: { 'X-Organization-Id': tenantId },
          query: { limit: 100 },
        },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar tracking session tidak tersedia')
      return data.data as TrackingSession[]
    },
  )
  const writableTargets = (targets.data ?? []).filter((target) =>
    canWriteTarget(target.id),
  )
  const writableDevices = (devices.data ?? []).filter(
    (device) => device.enabled && canWriteDevice(device.id),
  )
  const canManageAny =
    canCreateTarget ||
    canCreateDevice ||
    writableTargets.length > 0 ||
    (devices.data ?? []).some(
      (device) => canWriteDevice(device.id) || canRotateDevice(device.id),
    )
  const refresh = async () => {
    if (organizationId)
      await queryClient.invalidateQueries({
        queryKey: organizationQueryKey(organizationId),
      })
  }
  const createTarget = useMutation({
    mutationFn: async (values: z.infer<typeof targetSchema>) =>
      withPortalMutation(async () => {
        const { error } = await portalApi.POST('/targets', {
          params: { header: { 'X-Organization-Id': organizationId! } },
          body: {
            name: values.name,
            ...(values.external_ref
              ? { external_ref: values.external_ref }
              : {}),
          },
        })
        if (error) throw error
      }),
    onSuccess: async () => {
      setForm(null)
      setNotice('Target berhasil dibuat.')
      await refresh()
    },
  })
  const updateTarget = useMutation({
    mutationFn: async ({
      target,
      values,
    }: {
      target: Target
      values: z.infer<typeof targetSchema>
    }) =>
      withPortalMutation(async () => {
        const { error } = await portalApi.PATCH('/targets/{target_id}', {
          params: {
            header: { 'X-Organization-Id': organizationId! },
            path: { target_id: target.id },
          },
          body: {
            name: values.name,
            external_ref: values.external_ref || null,
          },
        })
        if (error) throw error
      }),
    onSuccess: async () => {
      setTargetToEdit(null)
      setNotice('Target berhasil diperbarui.')
      await refresh()
    },
  })
  const createDevice = useMutation({
    mutationFn: async (values: z.infer<typeof deviceSchema>) =>
      withPortalMutation(async () => {
        const { error } = await portalApi.POST('/devices', {
          params: { header: { 'X-Organization-Id': organizationId! } },
          body: values,
        })
        if (error) throw error
      }),
    onSuccess: async () => {
      setForm(null)
      setNotice(
        'Device terdaftar tanpa token. Terbitkan token secara terpisah saat siap.',
      )
      await refresh()
    },
  })
  const setDeviceEnabled = useMutation({
    mutationFn: async ({
      device,
      enabled,
    }: {
      device: Device
      enabled: boolean
    }) =>
      withPortalMutation(async () => {
        const { error } = await portalApi.PATCH('/devices/{device_id}', {
          params: {
            header: { 'X-Organization-Id': organizationId! },
            path: { device_id: device.id },
          },
          body: { enabled },
        })
        if (error) throw error
      }),
    onSuccess: refresh,
  })
  const startSession = useMutation({
    mutationFn: async (values: z.infer<typeof sessionSchema>) =>
      withPortalMutation(async () => {
        const { error } = await portalApi.POST('/sessions', {
          params: {
            header: {
              'X-Organization-Id': organizationId!,
              'Idempotency-Key': crypto.randomUUID(),
            },
          },
          body: values,
        })
        if (error) throw error
      }),
    onSuccess: async () => {
      setForm(null)
      setNotice('Tracking session berhasil dimulai.')
      await refresh()
    },
  })
  const endSession = useMutation({
    mutationFn: async (session: TrackingSession) =>
      withPortalMutation(async () => {
        const { error } = await portalApi.POST('/sessions/{session_id}/end', {
          params: {
            header: { 'X-Organization-Id': organizationId! },
            path: { session_id: session.id },
          },
          body: {},
        })
        if (error) throw error
      }),
    onSuccess: refresh,
  })

  const rotateCredentialNow = async (device: Device) => {
    setRotatingCredential(true)
    try {
      await withPortalMutation(async () => {
        const { data, error } = await portalApi.POST(
          '/devices/{device_id}/credentials/rotate',
          {
            params: {
              header: { 'X-Organization-Id': organizationId! },
              path: { device_id: device.id },
            },
            body: {},
          },
        )
        if (error || !data) throw error ?? new Error('Token tidak tersedia')
        // Keep the one-time secret out of React Query's query and mutation caches.
        setOneTimeToken({ device: device.name, token: data.token })
      })
      await refresh()
    } finally {
      setRotatingCredential(false)
    }
  }

  const mutationError =
    createTarget.error ??
    updateTarget.error ??
    createDevice.error ??
    setDeviceEnabled.error ??
    startSession.error ??
    endSession.error
  const busy =
    rotatingCredential ||
    createTarget.isPending ||
    updateTarget.isPending ||
    createDevice.isPending ||
    setDeviceEnabled.isPending ||
    startSession.isPending ||
    endSession.isPending
  return (
    <section className="mx-auto max-w-6xl space-y-7">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-muted-foreground text-sm">
            Provisioning & tracking
          </p>
          <h1 className="text-3xl font-semibold tracking-tight">
            Target, device & session
          </h1>
          <p className="text-muted-foreground mt-2 max-w-2xl text-sm">
            Daftarkan resource, kelola kredensial perangkat, dan mulai/akhiri
            sesi tracking pada organisasi aktif.
          </p>
        </div>
        {isAdmin || effectiveGrants.data ? (
          <div className="flex gap-2">
            {canCreateTarget && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => setForm('target')}
              >
                <Plus /> Target
              </Button>
            )}
            {canCreateDevice && (
              <Button size="sm" onClick={() => setForm('device')}>
                <Plus /> Device
              </Button>
            )}
          </div>
        ) : null}
      </header>
      {notice && (
        <p
          className="bg-background rounded-lg border p-3 text-sm"
          role="status"
        >
          {notice}
        </p>
      )}
      {mutationError && <FormError error={mutationError} />}
      {effectiveGrants.error && <FormError error={effectiveGrants.error} />}
      {(form === 'target' || targetToEdit) && (
        <TargetForm
          busy={busy}
          target={targetToEdit ?? undefined}
          onCancel={() => {
            setForm(null)
            setTargetToEdit(null)
          }}
          onSubmit={(value) =>
            targetToEdit
              ? updateTarget.mutate({ target: targetToEdit, values: value })
              : createTarget.mutate(value)
          }
        />
      )}
      {form === 'device' && (
        <DeviceForm
          busy={busy}
          allowAlways={!!isAdmin}
          onCancel={() => setForm(null)}
          onSubmit={(value) => createDevice.mutate(value)}
        />
      )}
      {form === 'session' && targets.data && devices.data && (
        <SessionForm
          busy={busy}
          targets={writableTargets}
          devices={writableDevices}
          onCancel={() => setForm(null)}
          onSubmit={(value) => startSession.mutate(value)}
        />
      )}
      {credentialDevice && (
        <CredentialRotationForm
          busy={busy}
          device={credentialDevice}
          onCancel={() => setCredentialDevice(null)}
          onSubmit={async (credentials) => {
            await withPortalMutation(async () => {
              const { error } = await portalApi.POST('/auth/reauthenticate', {
                body: {
                  password: credentials.password,
                  ...(credentials.code ? { code: credentials.code } : {}),
                },
              })
              if (error) throw error
            })
            await rotateCredentialNow(credentialDevice)
            setCredentialDevice(null)
          }}
        />
      )}
      {oneTimeToken && (
        <div
          className="rounded-xl border border-amber-500/50 bg-amber-500/5 p-4"
          role="alert"
        >
          <div className="flex items-start gap-3">
            <ShieldAlert className="mt-0.5 size-5" />
            <div className="min-w-0 flex-1">
              <h2 className="font-semibold">Token perangkat — tampil sekali</h2>
              <p className="text-muted-foreground my-2 text-sm">
                Simpan ke secret manager client sekarang. Token tidak disimpan
                browser dan tidak dapat ditampilkan kembali. Tutup panel untuk
                menghapusnya dari state.
              </p>
              <code className="bg-muted block max-h-24 overflow-auto rounded p-3 text-sm break-all">
                {oneTimeToken.token}
              </code>
              <p className="mt-2 text-xs">Device: {oneTimeToken.device}</p>
              <Button
                className="mt-3"
                size="sm"
                variant="outline"
                onClick={() => setOneTimeToken(null)}
              >
                Tutup dan hapus token
              </Button>
            </div>
          </div>
        </div>
      )}
      <div className="grid gap-6 xl:grid-cols-2">
        <section className="space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-xl font-semibold">Targets</h2>
            {targets.data?.length === 0 && (
              <span className="text-muted-foreground text-xs">Kosong</span>
            )}
          </div>
          {targets.isLoading && <p role="status">Memuat target…</p>}
          {targets.error && <FormError error={targets.error} />}
          {targets.data?.length === 0 && (
            <Empty text="Belum ada target di organisasi ini." />
          )}
          {targets.data?.map((target) => (
            <article
              className="bg-background rounded-xl border p-4"
              key={target.id}
            >
              <h3 className="font-medium">{target.name}</h3>
              <p className="text-muted-foreground mt-1 text-xs">
                {target.external_ref || 'Tanpa external reference'} ·{' '}
                {target.id}
              </p>
              {canWriteTarget(target.id) && (
                <Button
                  className="mt-3"
                  size="sm"
                  variant="outline"
                  onClick={() => setTargetToEdit(target)}
                >
                  Edit target
                </Button>
              )}
            </article>
          ))}
        </section>
        <section className="space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-xl font-semibold">Devices</h2>
            {writableTargets.length > 0 && writableDevices.length > 0 && (
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => setForm('session')}
              >
                <Radio /> Mulai session
              </Button>
            )}
          </div>
          {devices.isLoading && <p role="status">Memuat device…</p>}
          {devices.error && <FormError error={devices.error} />}
          {devices.data?.length === 0 && (
            <Empty text="Belum ada device terdaftar." />
          )}
          {devices.data?.map((device) => (
            <article
              className="bg-background rounded-xl border p-4"
              key={device.id}
            >
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h3 className="font-medium">
                    {device.name}{' '}
                    <span className="text-muted-foreground text-xs">
                      {device.enabled ? 'Aktif' : 'Nonaktif'}
                    </span>
                  </h3>
                  <p className="text-muted-foreground mt-1 text-xs">
                    {device.protocol} · {device.identifier} ·{' '}
                    {device.ingestion_policy}
                  </p>
                  <p className="text-muted-foreground mt-1 font-mono text-[11px] break-all">
                    {device.id}
                  </p>
                  {!device.enabled && (
                    <p className="mt-1 text-xs text-amber-700">
                      Ingestion berikutnya ditolak; session aktif dan histori
                      tidak dihapus.
                    </p>
                  )}
                </div>
                {(canRotateDevice(device.id) || canWriteDevice(device.id)) && (
                  <div className="flex flex-wrap gap-2">
                    {canRotateDevice(device.id) && (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!device.enabled || busy}
                        onClick={() => setCredentialDevice(device)}
                      >
                        <KeyRound /> Terbitkan / rotasi token
                      </Button>
                    )}
                    {canWriteDevice(device.id) && (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={busy}
                        onClick={() =>
                          setDeviceEnabled.mutate({
                            device,
                            enabled: !device.enabled,
                          })
                        }
                      >
                        {device.enabled ? 'Nonaktifkan' : 'Aktifkan'}
                      </Button>
                    )}
                  </div>
                )}
              </div>
            </article>
          ))}
        </section>
      </div>
      <section className="space-y-3">
        <h2 className="text-xl font-semibold">Tracking sessions</h2>
        {trackingSessions.isLoading && <p role="status">Memuat session…</p>}
        {trackingSessions.error && <FormError error={trackingSessions.error} />}
        {trackingSessions.data?.length === 0 && (
          <Empty text="Belum ada tracking session. Device session_only memerlukan session aktif sebelum posisi dapat tersimpan." />
        )}
        {trackingSessions.data?.map((session) => (
          <article
            className="bg-background flex flex-wrap items-center justify-between gap-3 rounded-xl border p-4"
            key={session.id}
          >
            <div>
              <p className="font-medium">
                {session.status === 'active' ? 'Aktif' : 'Selesai'} ·{' '}
                {targets.data?.find((target) => target.id === session.target_id)
                  ?.name ?? 'Target'}{' '}
                /{' '}
                {devices.data?.find((device) => device.id === session.device_id)
                  ?.name ?? 'Device'}
              </p>
              <p className="text-muted-foreground mt-1 text-xs">
                Mulai {new Date(session.started_at).toLocaleString()} ·{' '}
                {session.id}
              </p>
            </div>
            {session.status === 'active' &&
              canWriteTarget(session.target_id) &&
              canWriteDevice(session.device_id) && (
                <Button
                  size="sm"
                  variant="destructive"
                  disabled={busy}
                  onClick={() => endSession.mutate(session)}
                >
                  Akhiri session
                </Button>
              )}
          </article>
        ))}
      </section>
      {!canManageAny && !isAdmin && (
        <p className="text-muted-foreground text-xs">
          Akun ini memiliki akses baca. Aksi perubahan memerlukan role/grant
          yang sesuai; server tetap memvalidasi setiap operasi.
        </p>
      )}
    </section>
  )
}

function Empty({ text }: { text: string }) {
  return (
    <p className="text-muted-foreground rounded-xl border border-dashed p-5 text-sm">
      {text}
    </p>
  )
}

function CredentialRotationForm({
  busy,
  device,
  onCancel,
  onSubmit,
}: {
  busy: boolean
  device: Device
  onCancel: () => void
  onSubmit: (value: z.infer<typeof reauthenticationSchema>) => Promise<void>
}) {
  const form = useForm<z.infer<typeof reauthenticationSchema>>({
    resolver: zodResolver(reauthenticationSchema),
  })
  const [error, setError] = useState<unknown>(null)
  return (
    <form
      className="bg-background grid gap-3 rounded-xl border p-4 sm:grid-cols-2"
      onSubmit={form.handleSubmit(async (value) => {
        setError(null)
        try {
          await onSubmit(value)
        } catch (cause) {
          setError(cause)
        } finally {
          form.reset()
        }
      })}
    >
      <h3 className="font-medium sm:col-span-2">
        Konfirmasi autentikasi sebelum rotasi token — {device.name}
      </h3>
      <label className="grid gap-1 text-sm">
        Password saat ini
        <input
          className="bg-background h-9 rounded-md border px-3"
          type="password"
          autoComplete="current-password"
          {...form.register('password')}
        />
      </label>
      <label className="grid gap-1 text-sm">
        Kode MFA jika diminta
        <input
          className="bg-background h-9 rounded-md border px-3"
          autoComplete="one-time-code"
          inputMode="numeric"
          {...form.register('code')}
        />
      </label>
      <div className="flex gap-2 sm:col-span-2">
        <Button size="sm" disabled={busy} type="submit">
          Konfirmasi dan rotasi
        </Button>
        <Button size="sm" type="button" variant="outline" onClick={onCancel}>
          Batal
        </Button>
      </div>
      {Boolean(error) && (
        <div className="sm:col-span-2">
          <FormError error={error} />
        </div>
      )}
    </form>
  )
}

function TargetForm({
  busy,
  target,
  onCancel,
  onSubmit,
}: {
  busy: boolean
  target?: Target
  onCancel: () => void
  onSubmit: (value: z.infer<typeof targetSchema>) => void
}) {
  const form = useForm<z.infer<typeof targetSchema>>({
    resolver: zodResolver(targetSchema),
    defaultValues: {
      name: target?.name ?? '',
      external_ref: target?.external_ref ?? '',
    },
  })
  return (
    <InlineForm
      title={target ? 'Perbarui target' : 'Daftarkan target'}
      busy={busy}
      onCancel={onCancel}
      onSubmit={form.handleSubmit(onSubmit)}
    >
      <label className="grid gap-1 text-sm">
        Nama
        <input
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('name')}
        />
      </label>
      <label className="grid gap-1 text-sm">
        External reference (opsional)
        <input
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('external_ref')}
        />
      </label>
      {form.formState.errors.name && (
        <p className="text-destructive text-xs">Nama wajib 1–150 karakter.</p>
      )}
    </InlineForm>
  )
}
function DeviceForm({
  busy,
  allowAlways,
  onCancel,
  onSubmit,
}: {
  busy: boolean
  allowAlways: boolean
  onCancel: () => void
  onSubmit: (value: z.infer<typeof deviceSchema>) => void
}) {
  const form = useForm<z.infer<typeof deviceSchema>>({
    resolver: zodResolver(deviceSchema),
    defaultValues: { protocol: 'http-json', ingestion_policy: 'session_only' },
  })
  return (
    <InlineForm
      title="Registrasi device"
      busy={busy}
      onCancel={onCancel}
      onSubmit={form.handleSubmit(onSubmit)}
    >
      <label className="grid gap-1 text-sm">
        Nama
        <input
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('name')}
        />
      </label>
      <label className="grid gap-1 text-sm">
        Identifier
        <input
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('identifier')}
        />
      </label>
      <label className="grid gap-1 text-sm">
        Protocol
        <select
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('protocol')}
        >
          <option value="http-json">http-json</option>
        </select>
      </label>
      <label className="grid gap-1 text-sm">
        Kebijakan ingestion
        <select
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('ingestion_policy')}
        >
          <option value="session_only">
            session_only — perlu session aktif
          </option>
          {allowAlways && <option value="always">always</option>}
        </select>
      </label>
      <p className="text-muted-foreground text-xs sm:col-span-2">
        Registrasi tidak otomatis menerbitkan token atau memulai tracking
        session. Default mengikuti kebijakan aman session_only.
      </p>
    </InlineForm>
  )
}
function SessionForm({
  busy,
  targets,
  devices,
  onCancel,
  onSubmit,
}: {
  busy: boolean
  targets: Target[]
  devices: Device[]
  onCancel: () => void
  onSubmit: (value: z.infer<typeof sessionSchema>) => void
}) {
  const form = useForm<z.infer<typeof sessionSchema>>({
    resolver: zodResolver(sessionSchema),
  })
  return (
    <InlineForm
      title="Mulai tracking session"
      busy={busy}
      onCancel={onCancel}
      onSubmit={form.handleSubmit(onSubmit)}
    >
      <label className="grid gap-1 text-sm">
        Target
        <select
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('target_id')}
        >
          <option value="">Pilih target</option>
          {targets.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
      <label className="grid gap-1 text-sm">
        Device aktif
        <select
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('device_id')}
        >
          <option value="">Pilih device</option>
          {devices.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
      <p className="text-muted-foreground text-xs sm:col-span-2">
        Waktu mulai ditetapkan server. Device/target yang sudah memiliki session
        aktif atau interval yang bentrok akan ditolak dengan konflik yang
        ditampilkan.
      </p>
    </InlineForm>
  )
}
