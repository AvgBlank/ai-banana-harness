import { AlertOctagon, Cable, FileText, Globe, ShieldCheck, SlidersHorizontal } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { BASELINE_HOST, OVERRIDE_HOST, TRUST_HASH, TRUST_PATH, TRUST_RAW, TRUST_SUMMARY } from "@/mock/data";
import { useApp } from "@/state/store";

function Section({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return (
    <section className="rounded-[12px] border border-line bg-s2/60">
      <h3 className="flex items-center gap-2 border-b border-line px-3.5 py-2 text-[13px] font-medium">
        <span className="text-muted">{icon}</span>
        {title}
      </h3>
      <div className="space-y-2 px-3.5 py-3">{children}</div>
    </section>
  );
}

export function TrustDialog() {
  const { state, set } = useApp();
  const [raw, setRaw] = useState(false);
  const [ack, setAck] = useState(false);
  const restricted = useRef<HTMLButtonElement>(null);
  const close = () => set({ dialog: null });

  return (
    <Dialog open={state.dialog === "trust"} onOpenChange={(o) => !o && close()}>
      <DialogContent
        title="Trust this workspace's config?"
        description="This folder has its own configuration that can run commands and change where your API key is sent."
        className="w-[min(94vw,680px)]"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          restricted.current?.focus();
        }}
      >
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-3 pt-6">
          <div className="flex items-start gap-3 pr-8">
            <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[10px] bg-peel-soft text-peel">
              <ShieldCheck size={20} />
            </span>
            <div>
              <h2 className="text-[17px] font-semibold">Trust this workspace's config?</h2>
              <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
                <span className="font-mono text-[12px] text-fg">payments-api</span> has a <span className="font-mono text-[12px]">.banana-harness/config.toml</span>. It can start programs,
                loosen guardrails and redirect where your API key goes. Until you trust it, the workspace runs in restricted mode and the file is ignored.
              </p>
              <p className="mt-1 font-mono text-[11px] text-faint">
                {TRUST_PATH} · sha256 {TRUST_HASH}…
              </p>
            </div>
          </div>

          <div className="mt-5 space-y-3">
            <Section icon={<Cable size={15} />} title="Programs it would start (MCP servers)">
              {TRUST_SUMMARY.mcp.map((m) => (
                <div key={m.name} className="rounded-md border border-line bg-bg px-3 py-2">
                  <div className="flex items-center gap-2 text-[13px]">
                    <span className="font-medium">{m.name}</span>
                    {m.risky && (
                      <Badge tone="danger">
                        <AlertOctagon size={11} /> passes your API key
                      </Badge>
                    )}
                  </div>
                  <code className="mt-1 block break-all font-mono text-[12px] text-muted">$ {m.command}</code>
                  {m.env.map((e) => (
                    <code key={e} className="mt-0.5 block font-mono text-[12px] text-danger">
                      env {e}
                    </code>
                  ))}
                </div>
              ))}
            </Section>

            <Section icon={<SlidersHorizontal size={15} />} title="Guardrail changes">
              {TRUST_SUMMARY.permissions.map((p) => (
                <div key={p.rule} className="flex items-center gap-2 text-[13px]">
                  <code className="font-mono text-[12px]">{p.rule}</code>
                  <span className="text-faint">{p.from}</span>
                  <span className="text-faint">→</span>
                  <Badge tone="danger">{p.to}</Badge>
                </div>
              ))}
            </Section>

            <Section icon={<Globe size={15} />} title="Where your API key is sent">
              <div className="grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-1.5 text-[13px]">
                <span className="text-muted">DeepSeek</span>
                <span className="font-mono text-[12px]">
                  <span className="text-faint line-through">{BASELINE_HOST}</span> <span className="text-faint">→</span> <span className="text-danger">{OVERRIDE_HOST}</span>
                </span>
                <Badge tone="danger">different host</Badge>
                <span className="text-muted">Qwen</span>
                <span className="font-mono text-[12px]">dashscope.aliyuncs.com</span>
                <Badge tone="ok">unchanged</Badge>
              </div>
              <label className="mt-1 flex cursor-pointer items-start gap-2.5 rounded-md border border-danger/40 bg-danger-soft px-3 py-2.5 text-[13px]">
                <Checkbox checked={ack} onCheckedChange={(v) => setAck(v === true)} aria-label={`Send my API key to ${OVERRIDE_HOST}`} className="mt-0.5" />
                <span>
                  My API key will be sent to <span className="font-mono text-[12px] font-medium">{OVERRIDE_HOST}</span>, a host this workspace chose.
                </span>
              </label>
            </Section>

            <Section icon={<FileText size={15} />} title="Other overrides">
              <ul className="list-inside list-disc space-y-0.5 text-[13px] text-muted">
                {TRUST_SUMMARY.other.map((o) => (
                  <li key={o}>{o}</li>
                ))}
              </ul>
              <p className="text-[12px] text-faint">
                This folder's <span className="font-mono">AGENTS.md</span> and skills still guide the agent in restricted mode.
              </p>
            </Section>

            <button onClick={() => setRaw(!raw)} className="cursor-pointer text-[13px] text-accent-text hover:underline">
              {raw ? "Hide raw file" : "View raw file"}
            </button>
            {raw && <pre className="max-h-56 overflow-auto rounded-md border border-line bg-bg px-3.5 py-3 font-mono text-[12px] leading-relaxed text-muted">{TRUST_RAW}</pre>}
          </div>
        </div>

        <div className="flex items-center gap-2 border-t border-line bg-s2 px-6 py-3.5">
          <span className="text-[12px] text-faint">Trust applies to this exact file. If it changes, you'll be asked again.</span>
          <div className="ml-auto flex gap-2">
            <Button ref={restricted} variant="secondary" onClick={close}>
              Open in restricted mode
            </Button>
            <Button
              variant="primary"
              disabled={!ack}
              onClick={() => {
                set({ dialog: null, trustedWorkspaces: [...new Set([...state.trustedWorkspaces, "ws-payments"])] });
                setAck(false);
              }}
            >
              Trust and open
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
