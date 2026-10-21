import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { portalApi, portalErrorMessage, withPortalMutation } from '@/api/client'
import { ConfirmAction } from '@/components/confirm-action'
import { Button } from '@/components/ui/button'
import {
  organizationQueryKey,
  useOrganizationQuery,
  usePortalAuth,
} from '@/portal-context'
import type { components } from '@/api/generated'
import {
  RecentAuthConfirmation,
  type Reauthentication,
} from './recent-auth-confirmation'

type Membership = components['schemas']['ManagementMembership']
type Grant = components['schemas']['Grant']

const membershipSchema = z.object({
  role: z.enum(['admin', 'operator', 'viewer']),
  status: z.enum(['active', 'suspended']).optional(),
})

type PendingAction = { label: string; run: () => Promise<void> } | null

export function MemberAccessPanel({
  organizationId,
  isAdmin,
}: {
  organizationId: string
  isAdmin: boolean
}) {
  const auth = usePortalAuth()
  const queryClient = useQueryClient()
  const [cursorStack, setCursorStack] = useState<(string | null)[]>([null])
  const [editingMember, setEditingMember] = useState<string | null>(null)
  const [editingGrants, setEditingGrants] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [pendingAction, setPendingAction] = useState<PendingAction>(null)
  const cursor = cursorStack.at(-1) ?? null
  const members = useOrganizationQuery(
    organizationId,
    `members:${cursor ?? 'first'}`,
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET('/organization/members', {
        params: {
          header: { 'X-Organization-Id': tenantId },
          query: { limit: 50, ...(cursor ? { cursor } : {}) },
        },
        signal,
      })
      if (error || !data)
        throw error ?? new Error('Daftar anggota tidak tersedia')
      return data
    },
    isAdmin,
  )

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
  const invalidateManagement = async () => {
    await queryClient.invalidateQueries({
      queryKey: organizationQueryKey(organizationId),
    })
    await queryClient.invalidateQueries({ queryKey: ['my-organizations'] })
    await auth.refreshMe()
  }

  if (!isAdmin) return null
  return (
    <section className="bg-background space-y-4 rounded-xl border p-5">
      <header>
        <h2 className="text-xl font-semibold">Anggota dan akses</h2>
        <p className="text-muted-foreground text-sm">
          Ubah peran, status, dan grants anggota pada organisasi aktif. Server
          tetap memeriksa izin dan proteksi admin terakhir.
        </p>
      </header>
      {notice && (
        <p role="status" className="text-sm">
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
      {members.isLoading && <p role="status">Memuat anggota…</p>}
      {members.error && (
        <p role="alert" className="text-destructive text-sm">
          {portalErrorMessage(members.error)}
        </p>
      )}
      {members.data?.data.length === 0 && (
        <p className="text-muted-foreground rounded-lg border border-dashed p-4 text-sm">
          Belum ada anggota.
        </p>
      )}
      <div className="divide-y">
        {members.data?.data.map((member) => (
          <article className="space-y-3 py-4" key={member.id}>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h3 className="font-medium">
                  {member.user?.display_name ||
                    member.user?.email ||
                    member.user_id}
                  {member.user_id === auth.me?.user.id ? ' · Anda' : ''}
                </h3>
                <p className="text-muted-foreground text-sm">
                  {member.role} · {member.status}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    setEditingGrants(null)
                    setEditingMember(
                      editingMember === member.user_id ? null : member.user_id,
                    )
                  }}
                >
                  {editingMember === member.user_id
                    ? 'Tutup edit'
                    : 'Ubah peran/status'}
                </Button>
                {member.role !== 'admin' && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      setEditingMember(null)
                      setEditingGrants(
                        editingGrants === member.user_id
                          ? null
                          : member.user_id,
                      )
                    }}
                  >
                    {editingGrants === member.user_id
                      ? 'Tutup grants'
                      : 'Atur grants'}
                  </Button>
                )}
                <ConfirmAction
                  label="Cabut membership"
                  confirmation="Akses organisasi, grants, dan subscription anggota ini akan dicabut."
                  onConfirm={() =>
                    requestRecentAuthentication(
                      `Cabut membership ${member.user?.email ?? member.user_id}`,
                      async () => {
                        await withPortalMutation(async () => {
                          const { error } = await portalApi.DELETE(
                            '/organization/members/{user_id}',
                            {
                              params: {
                                header: { 'X-Organization-Id': organizationId },
                                path: { user_id: member.user_id },
                              },
                            },
                          )
                          if (error) throw error
                        })
                        setNotice(
                          'Membership dicabut. Proteksi admin terakhir diperiksa server.',
                        )
                        setEditingMember(null)
                        setEditingGrants(null)
                        await invalidateManagement()
                      },
                    )
                  }
                />
              </div>
            </div>
            {editingMember === member.user_id && (
              <MembershipEditor
                member={member}
                onCancel={() => setEditingMember(null)}
                onSubmit={(values) =>
                  requestRecentAuthentication(
                    `Ubah akses ${member.user?.email ?? member.user_id}`,
                    async () => {
                      await withPortalMutation(async () => {
                        const { error } = await portalApi.PATCH(
                          '/organization/members/{user_id}',
                          {
                            params: {
                              header: { 'X-Organization-Id': organizationId },
                              path: { user_id: member.user_id },
                            },
                            body: values,
                          },
                        )
                        if (error) throw error
                      })
                      setNotice('Peran/status anggota diperbarui.')
                      setEditingMember(null)
                      await invalidateManagement()
                    },
                  )
                }
              />
            )}
            {editingGrants === member.user_id && (
              <GrantEditor
                organizationId={organizationId}
                member={member}
                onCancel={() => setEditingGrants(null)}
                onSave={(grants) =>
                  requestRecentAuthentication(
                    `Ganti grants ${member.user?.email ?? member.user_id}`,
                    async () => {
                      await withPortalMutation(async () => {
                        const { error } = await portalApi.PUT(
                          '/organization/members/{user_id}/grants',
                          {
                            params: {
                              header: { 'X-Organization-Id': organizationId },
                              path: { user_id: member.user_id },
                            },
                            body: { grants },
                          },
                        )
                        if (error) throw error
                      })
                      setNotice('Grants anggota diperbarui.')
                      setEditingGrants(null)
                      await invalidateManagement()
                    },
                  )
                }
              />
            )}
          </article>
        ))}
      </div>
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
          disabled={!members.data?.next_cursor}
          onClick={() => {
            if (members.data?.next_cursor)
              setCursorStack((stack) => [...stack, members.data!.next_cursor])
          }}
        >
          Berikutnya
        </Button>
      </div>
    </section>
  )
}

function MembershipEditor({
  member,
  onCancel,
  onSubmit,
}: {
  member: Membership
  onCancel: () => void
  onSubmit: (values: z.infer<typeof membershipSchema>) => void
}) {
  const form = useForm<z.infer<typeof membershipSchema>>({
    resolver: zodResolver(membershipSchema),
    defaultValues: {
      role: member.role,
      ...(member.status === 'pending_mfa' ? {} : { status: member.status }),
    },
  })
  return (
    <form
      className="bg-muted/30 grid gap-3 rounded-lg border p-4 sm:grid-cols-[1fr_1fr_auto_auto]"
      onSubmit={form.handleSubmit(onSubmit)}
    >
      <label className="grid gap-1 text-sm">
        Peran
        <select
          className="bg-background h-9 rounded-md border px-3"
          {...form.register('role')}
        >
          <option value="admin">Admin (MFA wajib)</option>
          <option value="operator">Operator</option>
          <option value="viewer">Viewer</option>
        </select>
      </label>
      <label className="grid gap-1 text-sm">
        Status
        <select
          className="bg-background h-9 rounded-md border px-3"
          disabled={member.status === 'pending_mfa'}
          {...form.register('status')}
        >
          {member.status === 'pending_mfa' && (
            <option value="pending_mfa">
              Pending MFA — aktif setelah enrollment
            </option>
          )}
          <option value="active">Aktif</option>
          <option value="suspended">Ditangguhkan</option>
        </select>
      </label>
      {member.status === 'pending_mfa' && (
        <p className="text-muted-foreground text-xs sm:col-span-4">
          Status pending MFA hanya berubah melalui enrollment MFA yang berhasil.
        </p>
      )}
      <Button className="self-end" size="sm" type="submit">
        Simpan
      </Button>
      <Button
        className="self-end"
        size="sm"
        type="button"
        variant="outline"
        onClick={onCancel}
      >
        Batal
      </Button>
    </form>
  )
}

function GrantEditor({
  organizationId,
  member,
  onCancel,
  onSave,
}: {
  organizationId: string
  member: Membership
  onCancel: () => void
  onSave: (grants: Grant[]) => void
}) {
  const grantsQuery = useOrganizationQuery(
    organizationId,
    `grants:${member.user_id}`,
    async (tenantId, signal) => {
      const { data, error } = await portalApi.GET(
        '/organization/members/{user_id}/grants',
        {
          params: {
            header: { 'X-Organization-Id': tenantId },
            path: { user_id: member.user_id },
          },
          signal,
        },
      )
      if (error || !data)
        throw error ?? new Error('Grants anggota tidak tersedia')
      return data.grants
    },
  )
  const targets = useOrganizationQuery(
    organizationId,
    'grant-target-options',
    (tenantId, signal) => fetchTargetOptions(tenantId, signal),
  )
  const devices = useOrganizationQuery(
    organizationId,
    'grant-device-options',
    (tenantId, signal) => fetchDeviceOptions(tenantId, signal),
  )
  const [draft, setDraft] = useState<Grant[] | null>(null)
  const [filter, setFilter] = useState('')
  const activeGrants = draft ?? grantsQuery.data ?? []
  const text = filter.trim().toLowerCase()
  const targetRows = (targets.data ?? []).filter((item) =>
    `${item.name} ${item.external_ref ?? ''}`.toLowerCase().includes(text),
  )
  const deviceRows = (devices.data ?? []).filter((item) =>
    `${item.name} ${item.identifier}`.toLowerCase().includes(text),
  )
  const loading =
    grantsQuery.isLoading || targets.isLoading || devices.isLoading
  const error = grantsQuery.error ?? targets.error ?? devices.error

  const toggle = (
    type: 'organization' | 'target' | 'device',
    id: string,
    permission:
      | 'targets:create'
      | 'devices:create'
      | 'read'
      | 'write'
      | 'credentials:write',
    enabled: boolean,
  ) => {
    const current = (draft ?? grantsQuery.data ?? []).find(
      (grant) => grant.resource_type === type && grant.resource_id === id,
    )
    const permissions = new Set<string>(current?.permissions ?? [])
    if (enabled) {
      permissions.add(permission)
      if (
        type !== 'organization' &&
        (permission === 'write' || permission === 'credentials:write')
      )
        permissions.add('read')
    } else {
      permissions.delete(permission)
      if (type !== 'organization' && permission === 'read') {
        permissions.delete('write')
        permissions.delete('credentials:write')
      }
    }
    const updated = (draft ?? grantsQuery.data ?? []).filter(
      (grant) => !(grant.resource_type === type && grant.resource_id === id),
    )
    if (permissions.size > 0) {
      updated.push({
        resource_type: type,
        resource_id: id,
        permissions: [...permissions],
      } as Grant)
    }
    setDraft(updated)
  }
  const has = (type: Grant['resource_type'], id: string, permission: string) =>
    activeGrants.some(
      (grant) =>
        grant.resource_type === type &&
        grant.resource_id === id &&
        grant.permissions.includes(permission as never),
    )

  return (
    <section
      aria-label={`Grants ${member.user?.email ?? member.user_id}`}
      className="bg-muted/30 space-y-4 rounded-lg border p-4"
    >
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h4 className="font-medium">Grant akses objek</h4>
          <p className="text-muted-foreground text-xs">
            Write dan credentials:write otomatis mencakup read. Tidak ada akses
            wildcard.
          </p>
        </div>
        <label className="grid gap-1 text-xs">
          Cari target/device
          <input
            className="bg-background h-9 rounded-md border px-3 text-sm"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
        </label>
      </header>
      {loading && <p role="status">Memuat opsi grant…</p>}
      {error && (
        <p role="alert" className="text-destructive text-sm">
          {portalErrorMessage(error)}
        </p>
      )}
      {!loading && !error && (
        <div className="space-y-4">
          <div className="grid gap-2 rounded border p-3 sm:grid-cols-[1fr_auto_auto]">
            <span className="text-sm font-medium">Organisasi aktif</span>
            <PermissionCheckbox
              label="Buat target"
              checked={has('organization', organizationId, 'targets:create')}
              onChange={(value) =>
                toggle('organization', organizationId, 'targets:create', value)
              }
            />
            <PermissionCheckbox
              label="Buat device"
              checked={has('organization', organizationId, 'devices:create')}
              onChange={(value) =>
                toggle('organization', organizationId, 'devices:create', value)
              }
            />
          </div>
          <div className="space-y-2">
            <h5 className="text-sm font-medium">Targets</h5>
            {targetRows.map((target) => (
              <ResourceGrantRow
                key={target.id}
                label={target.name}
                type="target"
                id={target.id}
                has={has}
                toggle={toggle}
              />
            ))}
            {targetRows.length === 0 && (
              <p className="text-muted-foreground text-xs">
                Tidak ada target yang cocok.
              </p>
            )}
          </div>
          <div className="space-y-2">
            <h5 className="text-sm font-medium">Devices</h5>
            {deviceRows.map((device) => (
              <ResourceGrantRow
                key={device.id}
                label={`${device.name} · ${device.identifier}`}
                type="device"
                id={device.id}
                has={has}
                toggle={toggle}
              />
            ))}
            {deviceRows.length === 0 && (
              <p className="text-muted-foreground text-xs">
                Tidak ada device yang cocok.
              </p>
            )}
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={loading || !!error || !grantsQuery.data}
          onClick={() => onSave(activeGrants)}
        >
          Simpan grants
        </Button>
        <Button size="sm" variant="outline" onClick={onCancel}>
          Batal
        </Button>
      </div>
    </section>
  )
}

function PermissionCheckbox({
  label,
  checked,
  onChange,
}: {
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="flex items-center gap-2 text-xs">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  )
}

function ResourceGrantRow({
  label,
  type,
  id,
  has,
  toggle,
}: {
  label: string
  type: 'target' | 'device'
  id: string
  has: (type: Grant['resource_type'], id: string, permission: string) => boolean
  toggle: (
    type: 'organization' | 'target' | 'device',
    id: string,
    permission:
      | 'targets:create'
      | 'devices:create'
      | 'read'
      | 'write'
      | 'credentials:write',
    enabled: boolean,
  ) => void
}) {
  return (
    <div className="grid gap-2 rounded border p-3 sm:grid-cols-[1fr_auto_auto_auto]">
      <span className="truncate text-sm" title={label}>
        {label}
      </span>
      <PermissionCheckbox
        label="Baca"
        checked={has(type, id, 'read')}
        onChange={(value) => toggle(type, id, 'read', value)}
      />
      <PermissionCheckbox
        label="Ubah"
        checked={has(type, id, 'write')}
        onChange={(value) => toggle(type, id, 'write', value)}
      />
      {type === 'device' ? (
        <PermissionCheckbox
          label="Credential"
          checked={has(type, id, 'credentials:write')}
          onChange={(value) => toggle(type, id, 'credentials:write', value)}
        />
      ) : (
        <span />
      )}
    </div>
  )
}

async function fetchTargetOptions(organizationId: string, signal: AbortSignal) {
  const result = []
  let cursor: string | undefined
  do {
    const { data, error } = await portalApi.GET('/targets', {
      params: {
        header: { 'X-Organization-Id': organizationId },
        query: { limit: 100, ...(cursor ? { cursor } : {}) },
      },
      signal,
    })
    if (error || !data) throw error ?? new Error('Daftar target tidak tersedia')
    result.push(...data.data)
    cursor = data.next_cursor ?? undefined
  } while (cursor)
  return result
}

async function fetchDeviceOptions(organizationId: string, signal: AbortSignal) {
  const result = []
  let cursor: string | undefined
  do {
    const { data, error } = await portalApi.GET('/devices', {
      params: {
        header: { 'X-Organization-Id': organizationId },
        query: { limit: 100, ...(cursor ? { cursor } : {}) },
      },
      signal,
    })
    if (error || !data) throw error ?? new Error('Daftar device tidak tersedia')
    result.push(...data.data)
    cursor = data.next_cursor ?? undefined
  } while (cursor)
  return result
}
