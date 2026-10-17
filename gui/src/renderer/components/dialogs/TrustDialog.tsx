import { ShieldAlert, TriangleAlert } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { cn } from "@/lib/cn";
import type { TrustReview } from "../../../shared/api";

export type ReviewState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; review: TrustReview };

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mt-4">
      <h3 className="text-[12px] font-medium text-muted">{title}</h3>
      <ul className="mt-1.5 divide-y divide-line overflow-hidden rounded-[10px] border border-line">
        {children}
      </ul>
    </section>
  );
}

function Row({ children, danger }: { children: ReactNode; danger?: boolean }) {
  return <li className={cn("px-3 py-2 text-[12.5px]", danger && "bg-danger-soft")}>{children}</li>;
}

/** The destinations the user must approve: each provider whose key would go somewhere new. */
export function differingOrigins(review: TrustReview): string[] {
  const origins = review.summary?.providers.flatMap((p) =>
    p.differs && p.origin ? [p.origin] : [],
  );
  return [...new Set(origins ?? [])];
}

/**
 * What a workspace config would change, read by main from the file. Opening it restricted has the
 * focus; trusting needs an extra acknowledgement when the API key would be sent somewhere new.
 */
export function TrustDialog({
  open,
  state,
  busy,
  error,
  onRestricted,
  onTrust,
}: {
  open: boolean;
  state: ReviewState;
  busy: boolean;
  error: string | null;
  onRestricted: () => void;
  onTrust: (acknowledgeHosts: boolean) => void;
}) {
  const [raw, setRaw] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);
  const restricted = useRef<HTMLButtonElement>(null);
  const review = state.status === "ready" ? state.review : null;
  const hosts = review ? differingOrigins(review) : [];
  const summary = review?.summary ?? null;

  // A new review starts with the raw view closed and nothing approved.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the file's hash is the trigger
  useEffect(() => {
    setRaw(false);
    setAcknowledged(false);
  }, [review?.sha256]);

  return (
    <Dialog open={open}>
      <DialogContent
        title="Trust this workspace's config?"
        description="The folder has its own configuration. It is loaded only if you trust it."
        className="w-[min(92vw,640px)]"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          restricted.current?.focus();
        }}
      >
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-4 pt-6">
          <div className="flex items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-[10px] bg-peel-soft text-peel">
              <ShieldAlert size={19} />
            </span>
            <div className="min-w-0">
              <h2 className="text-[17px] font-semibold">Trust this workspace's config?</h2>
              {review && (
                <p
                  className="truncate font-mono text-[11.5px] text-faint"
                  title={review.configPath}
                >
                  {review.configPath} · {review.sha256.slice(0, 12)}
                </p>
              )}
            </div>
          </div>
          <p className="mt-3 text-[13.5px] leading-relaxed text-muted">
            Until you trust it, the workspace runs in restricted mode: its config is not loaded.
          </p>

          {state.status === "loading" && (
            <p className="mt-4 text-[13px] text-muted">Reading the config…</p>
          )}
          {state.status === "error" && (
            <p role="alert" className="mt-4 text-[13px] text-danger">
              {state.message}
            </p>
          )}

          {review?.parseError && (
            <p role="alert" className="mt-4 flex gap-2 text-[13px] text-warn">
              <TriangleAlert size={15} className="mt-0.5 shrink-0" />
              The config does not parse, so it would fail to load: {review.parseError}
            </p>
          )}

          {summary && !raw && (
            <>
              <Section title="MCP programs">
                {summary.mcp.length === 0 && <Row>None</Row>}
                {summary.mcp.map((s) => (
                  <Row key={s.name} danger={s.leaksKey || s.usesEnvRef}>
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{s.name}</span>
                      {s.preload && <Badge>starts at open</Badge>}
                    </div>
                    <code className="mt-0.5 block break-all font-mono text-[12px]">
                      {s.command.join(" ") || "(no command)"}
                    </code>
                    {s.envNames.length > 0 && (
                      <div className="mt-0.5 text-muted">Environment: {s.envNames.join(", ")}</div>
                    )}
                    {s.leaksKey && (
                      <div className="mt-1 text-danger">
                        Passes your API key ({"{env:AI_API_KEY}"}) to this program.
                      </div>
                    )}
                    {!s.leaksKey && s.usesEnvRef && (
                      <div className="mt-1 text-danger">
                        Passes an environment variable to this program.
                      </div>
                    )}
                  </Row>
                ))}
              </Section>

              <Section title="Guardrail changes">
                {summary.permissions.length === 0 && <Row>None</Row>}
                {summary.permissions.map((p) => (
                  <Row key={`${p.tool}:${p.pattern}`} danger={p.risky}>
                    <span className="font-mono">
                      {p.tool}
                      {p.pattern !== null && ` · ${p.pattern}`}
                    </span>{" "}
                    → <span className="font-medium">{p.action}</span>
                  </Row>
                ))}
              </Section>

              <Section title="Where your API key is sent">
                {summary.providers.length === 0 && <Row>No provider is changed.</Row>}
                {summary.providers.map((p) => (
                  <Row key={p.provider} danger={p.differs}>
                    <span className="font-medium">{p.provider}</span>{" "}
                    <span className="font-mono">{p.origin ?? p.baseUrl}</span>
                    {p.differs && (
                      <div className="mt-0.5 text-danger">
                        Differs from{" "}
                        {review?.baseline.find((b) => b.provider === p.provider)?.origin}.
                      </div>
                    )}
                  </Row>
                ))}
              </Section>

              {summary.other.length > 0 && (
                <Section title="Other overrides">
                  {summary.other.map((name) => (
                    <Row key={name}>
                      <span className="font-mono">{name}</span>
                    </Row>
                  ))}
                </Section>
              )}
            </>
          )}

          {review && raw && (
            <textarea
              readOnly
              aria-label="Raw config"
              value={review.raw}
              rows={12}
              className="mt-4 w-full resize-none rounded-[10px] border border-line bg-s2 p-3 font-mono text-[12px] leading-relaxed"
            />
          )}

          {review && (
            <button
              type="button"
              onClick={() => setRaw(!raw)}
              className="mt-3 cursor-pointer text-[12.5px] text-accent-text hover:underline"
            >
              {raw ? "Show summary" : "View raw file"}
            </button>
          )}

          <p className="mt-4 text-[12px] text-faint">
            The folder's AGENTS.md and skills still guide the agent in restricted mode.
          </p>

          {hosts.length > 0 && (
            <label className="mt-4 flex cursor-pointer items-start gap-2.5 text-[13px]">
              <input
                type="checkbox"
                checked={acknowledged}
                onChange={(e) => setAcknowledged(e.target.checked)}
                className="mt-0.5 h-4 w-4 accent-[var(--accent)]"
              />
              <span>My API key will be sent to {hosts.join(", ")}</span>
            </label>
          )}

          {error && (
            <p role="alert" className="mt-4 text-[13px] text-danger">
              {error}
            </p>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-line bg-s2 px-6 py-3.5">
          <Button ref={restricted} variant="primary" onClick={onRestricted} disabled={busy}>
            Open in restricted mode
          </Button>
          <Button
            variant="outline"
            disabled={
              busy || !review || review.parseError !== null || (hosts.length > 0 && !acknowledged)
            }
            onClick={() => onTrust(acknowledged)}
          >
            {busy ? "Trusting…" : "Trust and open"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
