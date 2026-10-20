export function PlaceholderPage({
  title,
  description,
}: {
  title: string
  description: string
}) {
  return (
    <section className="mx-auto max-w-5xl space-y-3">
      <p className="text-muted-foreground text-sm font-medium">Management</p>
      <h1 className="text-3xl font-semibold tracking-tight">{title}</h1>
      <p className="text-muted-foreground max-w-2xl">{description}</p>
      <p className="text-muted-foreground pt-3 text-sm">
        Belum ada data atau operasi yang tersedia.
      </p>
    </section>
  )
}
