const cliNames = ["Claude", "Codex", "Gemini", "Cursor", "VS Code"];

const profiles = [
  {
    name: "work",
    repo: "~/repos/company-api",
    env: "CLAUDE_CONFIG_DIR",
    status: "active",
  },
  {
    name: "personal",
    repo: "~/side-project",
    env: "CODEX_HOME",
    status: "ready",
  },
  {
    name: "client",
    repo: "~/client-audit",
    env: "GEMINI_CLI_HOME",
    status: "isolated",
  },
];

const pipeline = [
  ["01", "Find .cloak", "Walk from the repo to the filesystem root."],
  ["02", "Resolve profile", "Use the nearest marker or the configured fallback."],
  ["03", "Prepare env", "Point the CLI to the profile home and remove conflicting keys."],
  ["04", "Exec real CLI", "Replace the wrapper process with claude, codex or gemini."],
];

const capabilities = [
  {
    title: "Directory-scoped accounts",
    copy: "Each repository can carry its own identity without shell state or manual exports.",
    detail: "profile = work",
  },
  {
    title: "Credential cleanup",
    copy: "Known conflicting secrets leave the environment before the real CLI starts.",
    detail: "ANTHROPIC_API_KEY removed",
  },
  {
    title: "CLI-native MCP installs",
    copy: "Install servers with the native syntax of supported tools and keep them scoped.",
    detail: "cloak mcp install codex",
  },
  {
    title: "Doctor checks",
    copy: "Validate config, binaries, profile folders and account hints before you switch.",
    detail: "cloak doctor",
  },
];

function GlowLine() {
  return (
    <div className="mx-auto h-1 w-44 rounded-full bg-cloak-ember shadow-lg shadow-cloak-ember/50" />
  );
}

function TopNav() {
  return (
    <nav className="relative mx-auto h-16 w-full border border-cloak-paper/15 bg-cloak-paper rounded-full text-xs font-semibold text-cloak-paper shadow-2xl shadow-cloak-ember/10 sm:grid sm:h-auto sm:max-w-6xl sm:grid-cols-5 sm:items-center sm:px-5 sm:py-3">
      <a className="absolute left-5 top-1/2 -translate-y-1/2 transition  text-cloak-stage hover:text-cloak-stage/60 sm:static sm:justify-self-start sm:translate-y-0" href="#features">
        Features
      </a>
      <a className="hidden justify-self-center transition text-cloak-stage hover:text-cloak-stage/60 sm:block" href="#workflow">
        Workflow
      </a>
      <a
        className="absolute left-1/2 top-1/2 flex h-10 w-10 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-md border border-cloak-line bg-cloak-paper text-cloak-ink shadow-lg shadow-cloak-ember/10 sm:static sm:translate-x-0 sm:translate-y-0 sm:justify-self-center"
        href="#top"
        aria-label="cloak home"
      >
        <span className="h-3 w-3 rounded-sm bg-cloak-ink" />
      </a>
      <a className="absolute right-5 top-1/2 -translate-y-1/2 transition  text-cloak-stage hover:text-cloak-stage/60 sm:static sm:justify-self-center sm:translate-y-0" href="#install">
        Install
      </a>
      <a className="hidden justify-self-end transition  text-cloak-stage hover:text-cloak-stage/60 sm:block" href="#doctor">
        Doctor
      </a>
    </nav>
  );
}

function ProfilePill({ children }) {
  return (
    <span className="rounded-full border border-cloak-line bg-cloak-card px-3 py-1 text-xs font-semibold text-cloak-soft shadow-md shadow-cloak-ember/10">
      {children}
    </span>
  );
}

function GridBackdrop() {
  return (
    <div className="pointer-events-none absolute inset-0" aria-hidden="true">
      <div className="absolute inset-0 bg-gradient-to-b from-cloak-ink via-cloak-stage to-cloak-ink" />
      <div className="absolute inset-x-0 top-0 h-96 bg-gradient-to-b from-cloak-ember/25 via-cloak-ink/25 to-cloak-ink opacity-80" />
      <div className="absolute left-0 top-72 h-96 w-96 rounded-full bg-cloak-red-dark/25 blur-3xl" />
      <div className="absolute right-0 top-96 h-96 w-96 rounded-full bg-cloak-ember/20 blur-3xl" />
      <div className="absolute inset-0 opacity-25">
        <div className="absolute inset-y-0 left-1/4 border-l border-cloak-paper/20" />
        <div className="absolute inset-y-0 left-1/2 border-l border-cloak-paper/20" />
        <div className="absolute inset-y-0 left-3/4 border-l border-cloak-paper/20" />
        <div className="absolute inset-x-0 top-24 border-t border-cloak-paper/20" />
        <div className="absolute inset-x-0 top-48 border-t border-cloak-paper/20" />
        <div className="absolute inset-x-0 top-72 border-t border-cloak-paper/20" />
        <div className="absolute inset-x-0 top-96 border-t border-cloak-paper/20" />
      </div>
    </div>
  );
}

function WorkspaceMock() {
  return (
    <div className="relative mx-auto mt-10 w-full max-w-72 min-w-0 sm:mt-12 sm:max-w-5xl">
      <div className="absolute -left-4 top-16 z-20 hidden sm:block">
        <ProfilePill>Profile resolved</ProfilePill>
      </div>
      <div className="absolute -right-4 top-36 z-20 hidden sm:block">
        <ProfilePill>Env scrubbed</ProfilePill>
      </div>
      <div className="absolute left-8 top-72 z-20 hidden lg:block">
        <ProfilePill>No daemon</ProfilePill>
      </div>

      <section className="w-full max-w-full min-w-0 overflow-hidden rounded-lg border border-cloak-line bg-cloak-shell shadow-2xl shadow-cloak-ember/20">
        <div className="grid grid-cols-3 items-center border-b border-cloak-line bg-cloak-panel px-4 py-3 text-xs text-cloak-soft">
          <div className="flex items-center gap-2">
            <span className="h-2 w-2 rounded-full bg-cloak-ember" />
            <span className="h-2 w-2 rounded-full bg-cloak-muted" />
            <span className="h-2 w-2 rounded-full bg-cloak-line" />
          </div>
          <span className="text-center font-mono">cloak</span>
          <span className="hidden font-mono text-cloak-ember sm:inline">
            profile: work
          </span>
        </div>

        <div className="grid min-h-96 min-w-0 md:grid-cols-3">
          <aside className="min-w-0 border-b border-cloak-line bg-cloak-panel p-4 md:col-span-1 md:border-r md:border-b-0">
            <div className="mb-6">
              <p className="text-xs font-semibold uppercase tracking-wide text-cloak-muted">
                Active repo
              </p>
              <p className="mt-2 break-words font-mono text-sm text-cloak-text">
                ~/repos/company-api
              </p>
            </div>
            <div className="space-y-2">
              {profiles.map((profile) => (
                <div
                  key={profile.name}
                  className={`rounded-md border p-3 ${profile.status === "active"
                    ? "border-cloak-ember bg-cloak-ember/10"
                    : "border-cloak-line bg-cloak-card"
                    }`}
                >
                  <div className="flex min-w-0 flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
                    <p className="font-semibold text-cloak-text">{profile.name}</p>
                    <span className="font-mono text-xs uppercase text-cloak-muted">
                      {profile.status}
                    </span>
                  </div>
                  <p className="mt-2 break-all font-mono text-xs text-cloak-soft sm:break-words">
                    {profile.env}
                  </p>
                </div>
              ))}
            </div>
          </aside>

          <div className="min-w-0 overflow-hidden p-4 sm:p-5 md:col-span-2">
            <div className="mb-4 flex flex-col gap-3 border-b border-cloak-line pb-4 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <p className="text-xs font-semibold uppercase tracking-wide text-cloak-muted">
                  Profile session
                </p>
                <h2 className="mt-1 max-w-full break-words text-xl font-semibold text-cloak-text sm:text-2xl">
                  Company API opens as work
                </h2>
              </div>
              <a
                className="inline-flex items-center justify-center rounded-full bg-cloak-ember px-4 py-2 text-sm font-semibold text-cloak-ink shadow-lg shadow-cloak-ember/30 transition hover:bg-cloak-ember-soft"
                href="#install"
              >
                Install
              </a>
            </div>

            <div className="grid gap-3 lg:grid-cols-3">
              <div className="min-w-0 rounded-md border border-cloak-line bg-cloak-card p-4 lg:col-span-2">
                <p className="font-mono text-xs text-cloak-muted">$ cloak profile show</p>
                <div className="mt-4 space-y-3 font-mono text-sm">
                  <p className="text-cloak-text">profile = work</p>
                  <p className="break-words text-cloak-soft sm:hidden">
                    claude home = profiles/work
                  </p>
                  <p className="hidden break-all text-cloak-soft sm:block">
                    claude home = ~/.config/cloak/profiles/work/claude
                  </p>
                  <p className="text-cloak-ember">OPENAI_API_KEY removed</p>
                </div>
              </div>
              <div className="min-w-0 rounded-md border border-cloak-line bg-cloak-card p-4">
                <p className="text-xs font-semibold uppercase tracking-wide text-cloak-muted">
                  Account hint
                </p>
                <p className="mt-8 text-lg font-semibold text-cloak-text">
                  Jane Doe
                </p>
                <p className="mt-1 font-mono text-xs text-cloak-soft">
                  jane@company.dev
                </p>
              </div>
            </div>

            <div className="mt-3 grid gap-3 md:grid-cols-4">
              {pipeline.map(([step, title, copy]) => (
                <div key={step} className="rounded-md border border-cloak-line bg-cloak-card p-4">
                  <p className="font-mono text-xs text-cloak-ember">{step}</p>
                  <h3 className="mt-6 text-base font-semibold text-cloak-text">
                    {title}
                  </h3>
                  <p className="mt-2 text-sm leading-relaxed text-cloak-soft">
                    {copy}
                  </p>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

function CliStrip() {
  return (
    <div className="mx-auto max-w-5xl border-x border-b border-cloak-paper/10 bg-cloak-ink/35 px-4 pt-10 pb-8 text-center shadow-2xl shadow-cloak-ink/40">
      <p className="text-xs font-semibold uppercase tracking-wide text-cloak-paper/50">
        Routes local state for the tools you already use
      </p>
      <div className="mt-6 grid grid-cols-2 gap-3 sm:grid-cols-5">
        {cliNames.map((cli) => (
          <div
            key={cli}
            className="rounded-md border border-cloak-line bg-cloak-card px-4 py-3 text-sm font-semibold text-cloak-soft"
          >
            {cli}
          </div>
        ))}
      </div>
    </div>
  );
}

function FeatureGrid() {
  return (
    <section id="features" className="mx-auto max-w-6xl px-5 py-20 sm:px-8">
      <div className="grid gap-10 lg:grid-cols-2">
        <div>
          <p className="text-sm font-semibold text-cloak-ember">Faster context switching</p>
          <h2 className="mt-4 max-w-md text-4xl font-semibold leading-tight text-cloak-paper sm:text-5xl">
            Move between repos without carrying auth state in your shell.
          </h2>
          <p className="mt-5 max-w-md text-base leading-relaxed text-cloak-paper/70">
            `cloak` makes the active identity a property of the directory, then
            hands execution to the real CLI. The workflow stays familiar, the
            profile boundary becomes explicit.
          </p>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          {capabilities.map((item, index) => (
            <article
              key={item.title}
              className={`rounded-lg border border-cloak-line bg-cloak-card p-5 shadow-xl shadow-cloak-ink/30 ${index === 0 ? "sm:min-h-64" : ""
                } ${index === 2 ? "sm:col-span-2" : ""}`}
            >
              <p className="font-mono text-xs text-cloak-ember">{item.detail}</p>
              <h3 className="mt-8 text-xl font-semibold text-cloak-text">
                {item.title}
              </h3>
              <p className="mt-3 max-w-md text-sm leading-relaxed text-cloak-soft">
                {item.copy}
              </p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

function WorkflowSection() {
  return (
    <section id="workflow" className="mx-auto max-w-6xl px-5 pb-24 sm:px-8">
      <div className="text-center">
        <p className="text-sm font-semibold text-cloak-ember">What happens on exec</p>
        <h2 className="mt-4 text-4xl font-semibold leading-tight text-cloak-paper sm:text-5xl">
          One command, four local checks.
        </h2>
      </div>

      <div className="mt-12 grid gap-3 md:grid-cols-4">
        {pipeline.map(([step, title, copy]) => (
          <article key={step} className="rounded-lg border border-cloak-line bg-cloak-card p-5">
            <p className="font-mono text-sm text-cloak-ember">{step}</p>
            <h3 className="mt-12 text-xl font-semibold text-cloak-text">{title}</h3>
            <p className="mt-3 text-sm leading-relaxed text-cloak-soft">{copy}</p>
          </article>
        ))}
      </div>
    </section>
  );
}

function InstallSection() {
  return (
    <section id="install" className="mx-auto max-w-5xl px-5 pb-24 text-center sm:px-8">
      <div className="rounded-lg border border-cloak-line bg-cloak-card p-6 shadow-2xl shadow-cloak-ember/10 sm:p-10">
        <p className="text-sm font-semibold text-cloak-ember">Start with the local binary</p>
        <h2 className="mx-auto mt-4 max-w-3xl text-4xl font-semibold leading-tight text-cloak-text sm:text-5xl">
          Bind one repo to one profile, then let your normal CLI command do the rest.
        </h2>
        <div className="mx-auto mt-8 grid max-w-2xl gap-3 text-left font-mono text-sm">
          <p className="rounded-md border border-cloak-line bg-cloak-shell p-4 text-cloak-text">
            cargo install --path .
          </p>
          <p className="rounded-md border border-cloak-line bg-cloak-shell p-4 text-cloak-text">
            cloak profile create work
          </p>
          <p className="rounded-md border border-cloak-line bg-cloak-shell p-4 text-cloak-text">
            cloak use work
          </p>
          <p className="rounded-md border border-cloak-line bg-cloak-shell p-4 text-cloak-ember">
            cloak exec codex
          </p>
        </div>
      </div>
    </section>
  );
}

function DoctorSection() {
  return (
    <section id="doctor" className="mx-auto max-w-6xl px-5 pb-24 sm:px-8">
      <div className="grid gap-4 md:grid-cols-5">
        <div className="rounded-lg border border-cloak-line bg-cloak-card p-6 md:col-span-2">
          <p className="text-sm font-semibold text-cloak-ember">Doctor output</p>
          <h2 className="mt-4 text-3xl font-semibold leading-tight text-cloak-text">
            Make profile health visible before the wrong account opens.
          </h2>
        </div>
        <div className="rounded-lg border border-cloak-line bg-cloak-shell p-5 font-mono text-sm shadow-xl shadow-cloak-ink/30 md:col-span-3">
          <p className="text-cloak-muted">$ cloak doctor</p>
          <div className="mt-5 space-y-3">
            <p className="text-cloak-text">config.toml found</p>
            <p className="text-cloak-text">claude binary found</p>
            <p className="text-cloak-text">codex profile home hardened</p>
            <p className="text-cloak-ember">recommended gemini block missing</p>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home() {
  return (
    <main id="top" className="min-h-screen overflow-hidden bg-cloak-ink text-cloak-paper">
      <div className="absolute inset-x-0 top-0 h-px bg-cloak-line" aria-hidden="true" />
      <GridBackdrop />

      <div className="relative">
        <section className="pt-5">
          <div className="mx-auto w-full overflow-hidden border border-cloak-paper/10 bg-cloak-ink/70 p-2 shadow-2xl shadow-cloak-ember/10">
            <TopNav />
          </div>
        </section>

        <section className="mx-auto max-w-6xl px-5 pt-6 text-center sm:px-8">
          <div className="w-full max-w-full overflow-hidden rounded-3xl border border-cloak-paper/10 bg-cloak-ink/60 px-4 pt-10 pb-4 shadow-2xl shadow-cloak-ember/10 sm:px-8 sm:pt-12">
            <GlowLine />
            <div className="mt-12">
              <p className="mx-auto inline-flex max-w-xs rounded-full border border-cloak-line bg-cloak-card px-4 py-2 text-sm font-semibold text-cloak-soft shadow-lg shadow-cloak-ink/30">
                Local profile isolation for LLM CLIs
              </p>
              <h1 className="mx-auto mt-7 max-w-xs text-6xl font-semibold tracking-tight text-cloak-ember-soft sm:max-w-4xl sm:text-8xl">
                Cloak
              </h1>
              <p className="mx-auto mt-6 max-w-72 text-lg leading-relaxed text-cloak-paper/70 sm:max-w-2xl sm:text-xl">
                Open every repository with the right AI account. `cloak` resolves
                the directory profile, prepares isolated CLI homes, removes
                conflicting secrets and execs the real tool.
              </p>
            </div>

            <WorkspaceMock />
            <CliStrip />
          </div>
        </section>

        <FeatureGrid />
        <WorkflowSection />
        <DoctorSection />
        <InstallSection />

        <footer className="border-t border-cloak-line px-5 py-8 sm:px-8">
          <div className="mx-auto flex max-w-6xl flex-col gap-4 text-sm text-cloak-paper/50 sm:flex-row sm:items-center sm:justify-between">
            <p className="font-semibold text-cloak-paper">cloak</p>
            <p>Per-directory profiles for LLM CLIs and editors.</p>
          </div>
        </footer>
      </div>
    </main>
  );
}
