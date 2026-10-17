/** The empty shell: the rail and sidebar columns of the approved layout, with nothing in them yet. */
export function App() {
  return (
    <div className="flex h-full bg-bg text-fg">
      <nav aria-label="Sections" className="w-14 shrink-0 border-r border-line bg-s1" />
      <aside aria-label="Sessions" className="w-70 shrink-0 border-r border-line bg-s1" />
      <main className="flex min-w-0 flex-1 items-center justify-center">
        <p className="text-muted">Open a workspace to begin.</p>
      </main>
    </div>
  );
}
