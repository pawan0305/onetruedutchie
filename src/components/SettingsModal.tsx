import { useState } from "react";
import { api } from "../lib/tauri";
import type { SettingsView } from "../lib/types";

interface Props {
  settings: SettingsView | null;
  onSave: (deepgram: string, anthropic: string) => Promise<void> | void;
  onSettingsChanged: (s: SettingsView) => void;
  onClose: () => void;
  onError: (msg: string) => void;
}

export function SettingsModal({ settings, onSave, onSettingsChanged, onClose, onError }: Props) {
  const [dg, setDg] = useState("");
  const [an, setAn] = useState("");
  const [saving, setSaving] = useState(false);
  // Vocab state — one term per line in the textarea.
  const [vocabText, setVocabText] = useState<string>(
    (settings?.keywords ?? []).join("\n"),
  );
  const [savingVocab, setSavingVocab] = useState(false);

  const submit = async () => {
    setSaving(true);
    try {
      await onSave(dg, an);
    } finally {
      setSaving(false);
    }
  };

  const saveVocab = async () => {
    setSavingVocab(true);
    try {
      const words = vocabText
        .split("\n")
        .map((s) => s.trim())
        .filter((s) => s.length > 0);
      const s = await api.setVocab(words);
      onSettingsChanged(s);
    } catch (err) {
      onError(`vocab: ${err}`);
    } finally {
      setSavingVocab(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Settings</h2>
        <p className="muted">
          API keys live in <code>~/Library/Application Support</code>; nothing
          leaves your machine except provider API requests.
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
            · ~$0.0043/min for Nova-3 streaming
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

        <label>
          <span>
            Custom vocabulary
            <em className="muted"> (one word/phrase per line)</em>
          </span>
          <textarea
            value={vocabText}
            onChange={(e) => setVocabText(e.target.value)}
            placeholder={"names, jargon, or words Deepgram keeps mishearing\ne.g.\nKlaas\nDigiD\nABN Amro"}
            rows={5}
            spellCheck={false}
            autoComplete="off"
          />
          <small>
            Boosts these terms in Deepgram (Nova-3 <code>keyterm</code>). Takes
            effect on the next meeting.
          </small>
          <div style={{ marginTop: 6 }}>
            <button
              className="ghost"
              onClick={saveVocab}
              disabled={savingVocab}
            >
              {savingVocab ? "Saving…" : "Save vocabulary"}
            </button>
          </div>
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
