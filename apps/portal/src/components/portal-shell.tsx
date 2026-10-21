import { useQuery } from '@tanstack/react-query'
import {
  Activity,
  Building2,
  Clock3,
  LogOut,
  MapPinned,
  ShieldCheck,
  UsersRound,
} from 'lucide-react'
import { Link, Outlet, useLocation } from 'react-router'
import { portalApi, portalErrorMessage } from '@/api/client'
import { usePortalAuth, usePortalTenant } from '@/portal-context'
import { Button } from '@/components/ui/button'

const navigation = [
  { href: '/overview', label: 'Ringkasan', icon: Activity },
  { href: '/organization', label: 'Organisasi', icon: UsersRound },
  { href: '/sessions', label: 'Sesi login', icon: Clock3 },
  { href: '/devices', label: 'Devices', icon: MapPinned },
  { href: '/live', label: 'Peta live', icon: MapPinned },
  { href: '/history', label: 'Peta & histori', icon: MapPinned },
]

export function PortalShell() {
  const location = useLocation()
  const auth = usePortalAuth()
  const tenant = usePortalTenant()
  const organizations = useQuery({
    queryKey: ['my-organizations'],
    enabled: auth.status === 'authenticated',
    queryFn: async () => {
      const { data, error } = await portalApi.GET('/organizations', {
        params: { query: { limit: 100 } },
      })
      if (error || !data) throw error ?? new Error('Organisasi tidak tersedia')
      return data.data
    },
  })
  const currentOrganization = organizations.data?.find(
    (org) => org.id === tenant.organizationId,
  )

  return (
    <div className="bg-muted/30 text-foreground min-h-screen">
      <div className="mx-auto flex min-h-screen max-w-screen-2xl flex-col md:flex-row">
        <aside className="bg-background shrink-0 border-b p-3 md:w-64 md:border-r md:border-b-0 md:p-5">
          <Link className="flex items-center gap-3 px-2 py-3" to="/overview">
            <span className="bg-primary text-primary-foreground grid size-9 place-items-center rounded-xl">
              <MapPinned aria-hidden="true" className="size-5" />
            </span>
            <span>
              <span className="block font-semibold tracking-tight">
                Tracker
              </span>
              <span className="text-muted-foreground block text-xs">
                Management portal
              </span>
            </span>
          </Link>
          <nav
            aria-label="Navigasi utama"
            className="mt-3 flex gap-1 overflow-x-auto md:mt-10 md:flex-col md:overflow-visible"
          >
            {[
              ...navigation,
              ...(auth.me?.platform_admin
                ? [
                    {
                      href: '/platform/organizations',
                      label: 'Organisasi platform',
                      icon: ShieldCheck,
                    },
                  ]
                : []),
            ].map(({ href, label, icon: Icon }) => {
              const active = location.pathname === href
              return (
                <Link
                  aria-current={active ? 'page' : undefined}
                  className={`flex shrink-0 items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors ${active ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-muted hover:text-foreground'}`}
                  key={href}
                  to={href}
                >
                  <Icon aria-hidden="true" className="size-4" />
                  {label}
                </Link>
              )
            })}
          </nav>
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          <header className="bg-background flex min-h-16 flex-wrap items-center justify-between gap-3 border-b px-5 py-3 sm:px-8">
            <div className="flex items-center gap-2 md:hidden">
              <MapPinned aria-hidden="true" className="size-5" />
              <span className="font-semibold">Tracker</span>
            </div>
            <label className="flex items-center gap-2 text-sm">
              <Building2 className="text-muted-foreground size-4" />
              <span className="sr-only">Organisasi aktif</span>
              <select
                aria-label="Organisasi aktif"
                className="bg-background max-w-64 rounded-md border px-2 py-1.5"
                value={tenant.organizationId ?? ''}
                disabled={!organizations.data?.length}
                onChange={(event) => {
                  void tenant.selectOrganization(event.target.value)
                }}
              >
                {!organizations.data?.length && (
                  <option value="">Tidak ada organisasi aktif</option>
                )}
                {organizations.data?.map((org) => (
                  <option key={org.id} value={org.id}>
                    {org.name}
                  </option>
                ))}
              </select>
            </label>
            {organizations.error && (
              <span role="status" className="text-destructive text-xs">
                {portalErrorMessage(organizations.error)}
              </span>
            )}
            <div className="flex items-center gap-3">
              <span className="text-muted-foreground hidden text-sm sm:inline">
                {auth.me?.user.display_name || auth.me?.user.email}
              </span>
              <Button
                aria-label="Keluar"
                size="sm"
                variant="outline"
                onClick={() => void auth.logout().catch(() => undefined)}
              >
                <LogOut className="size-4" />
                <span className="hidden sm:inline">Keluar</span>
              </Button>
            </div>
          </header>
          <main className="flex-1 p-5 sm:p-8">
            <Outlet />
          </main>
        </div>
      </div>
      <span className="sr-only" data-testid="active-organization-name">
        {currentOrganization?.name ?? ''}
      </span>
    </div>
  )
}
