import { useState } from "react";
import type { SettingsView } from "../lib/types";

interface Props {
  settings: SettingsView | null;
  onSave: (deepgram: string, anthropic: string) => Promise<void> | void;
  onClose: () => void;
}

export function SettingsModal({ settings, onSave, onClose }: Props) {
  const [dg, setDg] = useState("");
  const [an, setAn] = useState("");
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    setSaving(true);
    try {
      await onSave(dg, an);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Settings</h2>
        <p className="muted">
          API keys are stored in the macOS Keychain — they never leave your
          machine except in the network requests OneTrueDutchie makes to the
          providers.
        </p>

        <label>
          <span>
            Deepgram API key
            {settings?.deepgram_set && (
              <em className="muted"> (currently set, leave blank to keep)</em>
            )}
          </span>
          <input
            type="password"
            value={dg}
            onChange={(e) => setDg(e.target.value)}
            placeholder="dg_..."
            autoComplete="off"
          />
          <small>
            <a
              href="https://console.deepgram.com/"
              target="_blank"
              rel="noreferrer"
            >
              console.deepgram.com
            </a>{" "}
            · ~$0.0043/min for Nova-2 streaming
          </small>
        </label>

        <label>
          <span>
            Anthropic API key
            {settings?.anthropic_set && (
              <em className="muted"> (currently set, leave blank to keep)</em>
            )}
          </span>
          <input
            type="password"
            value={an}
            onChange={(e) => setAn(e.target.value)}
            placeholder="sk-ant-..."
            autoComplete="off"
          />
          <small>
            <a
              href="https://console.anthropic.com/"
              target="_blank"
              rel="noreferrer"
            >
              console.anthropic.com
            </a>{" "}
            · Claude Haiku 4.5 ($1/MTok in, $5/MTok out)
          </small>
        </label>

        <div className="modal-actions">
          <button onClick={onClose}>Close</button>
          <button className="primary" onClick={submit} disabled={saving}>
            {saving ? "Saving…" : "Save keys"}
          </button>
        </div>
      </div>
    </div>
  );
}
