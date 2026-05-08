interface Props {
  summary: string | null;
  updatedAt: string | null;
  onRegenerate?: () => void;
}

export function SummaryPane({ summary, updatedAt, onRegenerate }: Props) {
  return (
    <section className="pane summary-pane">
      <header className="pane-header">
        <h2>Summary</h2>
        <div className="pane-sub-row">
          <span className="pane-sub">
            {updatedAt
              ? `updated ${new Date(updatedAt).toLocaleTimeString()}`
              : "auto-refreshes every 2 min"}
          </span>
          {onRegenerate && (
            <button className="ghost" onClick={onRegenerate}>
              ↻ refresh
            </button>
          )}
        </div>
      </header>
      <div className="pane-body scroll">
        {summary ? (
          <pre className="summary-text">{summary}</pre>
        ) : (
          <div className="empty">
            No summary yet. The first one is generated about two minutes after the
            meeting starts.
          </div>
        )}
      </div>
    </section>
  );
}
