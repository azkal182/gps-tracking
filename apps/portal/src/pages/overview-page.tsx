import { portalApi, portalErrorMessage } from '@/api/client'
import {
  usePortalAuth,
  usePortalTenant,
  useOrganizationQuery,
} from '@/portal-context'

export default function OverviewPage() {
  const auth = usePortalAuth()
  const tenant = usePortalTenant()
  const organization = useOrganizationQuery(
    tenant.organizationId,
    'detail',
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/organization', {
        params: { header: { 'X-Organization-Id': tenantId } },
        signal,
      })
      if (error || !data) throw error ?? new Error('Organisasi tidak tersedia')
      if (data.tenant_id !== tenantId || data.id !== tenantId)
        throw new Error(
          'Konteks organisasi berubah. Pilih ulang organisasi aktif.',
        )
      return data
    },
  )
  return (
    <section className="mx-auto max-w-5xl space-y-8">
      <header className="space-y-2">
        <p className="text-muted-foreground text-sm font-medium">Management</p>
        <h1 className="text-3xl font-semibold tracking-tight">Ringkasan</h1>
        <p className="text-muted-foreground max-w-2xl">
          Selamat datang, {auth.me?.user.display_name || auth.me?.user.email}.
          Data yang tampil mengikuti organisasi aktif pada tab ini.
        </p>
      </header>
      {organization.isLoading && <p role="status">Memuat organisasi…</p>}
      {organization.error && (
        <p role="alert" className="text-destructive text-sm">
          {portalErrorMessage(organization.error)}
        </p>
      )}
      <div className="grid gap-4 sm:grid-cols-2">
        <article className="bg-background rounded-xl border p-5 shadow-sm">
          <p className="text-sm font-medium">Organisasi aktif</p>
          <p className="mt-2 text-2xl font-semibold">
            {organization.data?.name ?? 'Belum dipilih'}
          </p>
          <p className="text-muted-foreground mt-2 text-sm">
            {organization.data?.status ??
              'Pilih organisasi yang keanggotaannya aktif.'}
          </p>
        </article>
        <article className="bg-background rounded-xl border p-5 shadow-sm">
          <p className="text-sm font-medium">Akun</p>
          <p className="mt-2 text-lg font-semibold">{auth.me?.user.email}</p>
          <p className="text-muted-foreground mt-2 text-sm">
            MFA{' '}
            {auth.me?.mfa_enabled
              ? 'aktif'
              : auth.me?.mfa_required
                ? 'wajib'
                : 'opsional'}
          </p>
        </article>
      </div>
    </section>
  )
}
