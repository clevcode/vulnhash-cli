import { useEffect, useState } from "react";

interface Entry {
  hash: string;
  local_id: number;
  parent?: string;
  algorithm: string;
  submitted_at: string;
  title: string;
  description: string;
  published: boolean;
  synced: boolean;
}

interface NoteEntry {
  written_at: string;
  content: string;
}

const api = {
  listEntries: (): Promise<Entry[]> =>
    fetch("/api/entries").then((r) => r.json()),
  getEntry: (hash: string): Promise<Entry> =>
    fetch(`/api/entries/${hash}`).then((r) => r.json()),
  getProof: (hash: string): Promise<string> =>
    fetch(`/api/entries/${hash}/proof`).then((r) => r.text()),
  listNotes: (hash: string): Promise<NoteEntry[]> =>
    fetch(`/api/entries/${hash}/notes`).then((r) => r.json()),
  addNote: (hash: string, content: string) =>
    fetch(`/api/entries/${hash}/notes`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content }),
    }).then((r) => r.json()),
  setDetails: (hash: string, title: string, description: string) =>
    fetch(`/api/entries/${hash}/details`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ title, description }),
    }).then((r) => r.json()),
  publish: (hash: string) =>
    fetch(`/api/entries/${hash}/publish`, { method: "POST" }).then((r) =>
      r.json(),
    ),
  getConfig: (): Promise<{ server: string; auth_token: string }> =>
    fetch("/api/config").then((r) => r.json()),
  setConfig: (data: { server?: string; auth_token?: string }) =>
    fetch("/api/config", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    }).then((r) => r.json()),
};

type View = "list" | "detail" | "config";

/// Read the current view out of the URL fragment.
///
/// Routing through the fragment rather than component state means a finding
/// can be linked, bookmarked and reloaded — and that the back button does what
/// it looks like it does.
function routeFromHash(): { view: View; hash: string } {
  const fragment = window.location.hash.slice(1);
  if (fragment === "/config") return { view: "config", hash: "" };
  const match = fragment.match(/^\/f\/([a-fA-F0-9]+)$/);
  if (match) return { view: "detail", hash: match[1] };
  return { view: "list", hash: "" };
}

export default function App() {
  const [route, setRoute] = useState(routeFromHash);

  useEffect(() => {
    const onChange = () => setRoute(routeFromHash());
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);

  const view = route.view;
  const selectedHash = route.hash;
  const go = (fragment: string) => {
    window.location.hash = fragment;
  };
  const showDetail = (hash: string) => go(`/f/${hash}`);
  const setView = (v: View) => go(v === "config" ? "/config" : "/");

  return (
    <div style={styles.app}>
      <header style={styles.header}>
        <h1 style={styles.brand} onClick={() => setView("list")}>
          <span style={styles.hash}>#</span> VulnHash
        </h1>
        <nav style={styles.nav}>
          <button style={styles.navLink} onClick={() => setView("list")}>
            Findings
          </button>
          <button style={styles.navLink} onClick={() => setView("config")}>
            Config
          </button>
        </nav>
      </header>
      <main style={styles.main}>
        {view === "list" && <EntryList onSelect={showDetail} />}
        {view === "detail" && (
          <EntryDetail hash={selectedHash} onBack={() => go("/")} />
        )}
        {view === "config" && <Config />}
      </main>
    </div>
  );
}

function EntryList({ onSelect }: { onSelect: (hash: string) => void }) {
  const [entries, setEntries] = useState<Entry[]>([]);

  useEffect(() => {
    api.listEntries().then(setEntries);
  }, []);

  return (
    <div>
      <h2 style={styles.heading}>Local Findings ({entries.length})</h2>
      {entries.length === 0 ? (
        <p style={styles.muted}>
          No findings yet. Use <code>vulnhash submit</code> to add one.
        </p>
      ) : (
        <div style={styles.list}>
          {entries.map((e) => (
            <div
              key={e.hash}
              style={styles.card}
              onClick={() => onSelect(e.hash)}
            >
              <span style={styles.localId}>{e.local_id}</span>
              <code style={styles.hashCode}>{e.hash.substring(0, 24)}...</code>
              {e.parent && <span style={styles.followUp}>follow-up</span>}
              <span
                style={{
                  ...styles.badge,
                  background: e.published
                    ? "rgba(34,197,94,0.15)"
                    : "rgba(136,136,160,0.15)",
                  color: e.published ? "#22c55e" : "#8888a0",
                }}
              >
                {e.published ? "published" : "private"}
              </span>
              {e.synced && (
                <span
                  style={{
                    ...styles.badge,
                    background: "rgba(0,212,170,0.15)",
                    color: "#00d4aa",
                  }}
                >
                  synced
                </span>
              )}
              <span style={styles.date}>
                {new Date(e.submitted_at).toLocaleDateString()}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function EntryDetail({
  hash,
  onBack,
}: {
  hash: string;
  onBack: () => void;
}) {
  const [entry, setEntry] = useState<Entry | null>(null);
  const [proof, setProof] = useState("");
  const [notes, setNotes] = useState<NoteEntry[]>([]);
  const [newNote, setNewNote] = useState("");
  const [publishing, setPublishing] = useState(false);
  const [message, setMessage] = useState("");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [savingDetails, setSavingDetails] = useState(false);
  const [detailsMessage, setDetailsMessage] = useState("");

  useEffect(() => {
    api.getEntry(hash).then((e) => {
      setEntry(e);
      setTitle(e.title || "");
      setDescription(e.description || "");
    });
    api.getProof(hash).then(setProof);
    api.listNotes(hash).then(setNotes);
  }, [hash]);

  const handleSaveDetails = async () => {
    setSavingDetails(true);
    setDetailsMessage("");
    try {
      const result = await api.setDetails(hash, title, description);
      if (result.error) {
        setDetailsMessage("Saved locally, but the server refused: " + result.error);
      } else if (result.warning) {
        setDetailsMessage(result.warning);
      } else {
        setDetailsMessage("Saved and visible.");
      }
      api.getEntry(hash).then(setEntry);
    } catch {
      setDetailsMessage("Could not save.");
    }
    setSavingDetails(false);
  };

  const handleAddNote = async () => {
    if (!newNote.trim()) return;
    await api.addNote(hash, newNote);
    setNewNote("");
    api.listNotes(hash).then(setNotes);
  };

  const handlePublish = async () => {
    setPublishing(true);
    try {
      const result = await api.publish(hash);
      if (result.error) {
        setMessage("Error: " + result.error);
      } else {
        setMessage("Published successfully!");
        api.getEntry(hash).then(setEntry);
      }
    } catch {
      setMessage("Publish failed");
    }
    setPublishing(false);
  };

  if (!entry) return <p style={styles.muted}>Loading...</p>;

  return (
    <div>
      <button style={styles.backBtn} onClick={onBack}>
        &larr; All findings
      </button>

      <div style={styles.detailHead}>
        <span style={styles.detailId}>{entry.local_id}</span>
        <code style={styles.detailHash}>{entry.hash}</code>
      </div>
      <div style={styles.chipRow}>
        <span style={entry.published ? styles.chipPublished : styles.chipSealed}>
          {entry.published ? "disclosed" : "sealed"}
        </span>
        <span style={entry.synced ? styles.chipOk : styles.chipWarn}>
          {entry.synced ? "registered with the server" : "local only"}
        </span>
        <span style={styles.chipQuiet}>{entry.algorithm}</span>
        <span style={styles.chipQuiet}>
          {new Date(entry.submitted_at).toLocaleString()}
        </span>
      </div>
      {entry.parent && (
        <p style={styles.parentLine}>
          Follows up on <code style={styles.inlineHash}>{entry.parent}</code>
        </p>
      )}

      <Section
        title="Proof document"
        note="The exact bytes that were hashed. Publishing reveals this verbatim, and anyone can re-hash it to check it is what you registered."
      >
        <pre style={styles.proof}>{proof}</pre>
        <p style={styles.byteCount}>
          {new Blob([proof]).size} bytes &rarr; sha256 {entry.hash.slice(0, 16)}...
        </p>
      </Section>

      <Section
        title="Public details"
        note="Visible to anyone browsing your findings, whether or not you have disclosed the document. Use it to stake out an area without saying what you found."
      >
        <input
          style={styles.input}
          placeholder="Title, e.g. Heap overflow in a widely used parser"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <textarea
          style={{ ...styles.textarea, marginTop: "0.5rem" }}
          placeholder="Anything you are willing to say publicly before disclosing."
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={3}
        />
        <button style={styles.secondaryBtn} onClick={handleSaveDetails} disabled={savingDetails}>
          {savingDetails ? "Saving..." : "Save public details"}
        </button>
        {detailsMessage && <p style={styles.message}>{detailsMessage}</p>}
      </Section>

      {!entry.published && (
        <Section
          title="Disclose"
          note="Publishing uploads the proof document and makes it public. It cannot be undone."
        >
          <button style={styles.publishBtn} onClick={handlePublish} disabled={publishing}>
            {publishing ? "Publishing..." : "Publish this finding"}
          </button>
          {message && <p style={styles.message}>{message}</p>}
        </Section>
      )}

      <Section
        title={`Private notes (${notes.length})`}
        note="Working notes for you. They stay in ~/.vulnhash on this machine and are never uploaded, published or hashed."
      >
        {notes.map((n) => (
          <div key={n.written_at} style={styles.noteCard}>
            <p style={styles.noteWhen}>{formatNoteTime(n.written_at)}</p>
            <pre style={styles.noteContent}>{n.content}</pre>
          </div>
        ))}
        <textarea
          style={styles.textarea}
          placeholder="Add a note..."
          value={newNote}
          onChange={(e) => setNewNote(e.target.value)}
          rows={3}
        />
        <button style={styles.secondaryBtn} onClick={handleAddNote}>
          Add note
        </button>
      </Section>
    </div>
  );
}

/// A labelled section with a line saying what it is for.
///
/// The old layout was a stack of headings and boxes that each assumed you knew
/// what they meant. The distinction that matters here — what stays on this
/// machine versus what the world can see — is not something to leave implicit.
function Section({
  title,
  note,
  children,
}: {
  title: string;
  note: string;
  children: React.ReactNode;
}) {
  return (
    <section style={styles.section}>
      <h3 style={styles.sectionTitle}>{title}</h3>
      <p style={styles.sectionNote}>{note}</p>
      {children}
    </section>
  );
}

function formatNoteTime(value: string) {
  const parsed = new Date(value);
  return isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function Config() {
  const [server, setServer] = useState("");
  const [token, setToken] = useState("");
  const [currentConfig, setCurrentConfig] = useState({ server: "", auth_token: "" });
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.getConfig().then((c) => {
      setCurrentConfig(c);
      setServer(c.server);
    });
  }, []);

  const handleSave = async () => {
    const data: { server?: string; auth_token?: string } = {};
    if (server !== currentConfig.server) data.server = server;
    if (token) data.auth_token = token;
    await api.setConfig(data);
    setToken("");
    setSaved(true);
    api.getConfig().then(setCurrentConfig);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div>
      <h2 style={styles.heading}>Configuration</h2>
      <div style={styles.detailCard}>
        <p style={styles.muted}>
          Current token: {currentConfig.auth_token || "(not set)"}
        </p>
        <label style={styles.label}>
          Server URL
          <input
            style={styles.input}
            value={server}
            onChange={(e) => setServer(e.target.value)}
          />
        </label>
        <label style={styles.label}>
          Auth Token (paste new token to update)
          <input
            style={styles.input}
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="vh_..."
          />
        </label>
        <button style={styles.publishBtn} onClick={handleSave}>
          {saved ? "Saved!" : "Save"}
        </button>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  app: {
    // dvh, not vh: on mobile browsers vh counts the address bar, which pushes
    // the page over the viewport by exactly the height of a chrome that is not
    // there.
    minHeight: "100dvh",
    background: "#0a0a0f",
    color: "#e0e0e8",
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "1rem 2rem",
    borderBottom: "1px solid #2a2a3a",
    background: "#12121a",
  },
  brand: {
    fontSize: "1.3rem",
    fontWeight: 700,
    cursor: "pointer",
    margin: 0,
  },
  hash: { color: "#00d4aa" },
  nav: { display: "flex", gap: "0.5rem" },
  navLink: {
    background: "none",
    border: "none",
    color: "#8888a0",
    cursor: "pointer",
    padding: "0.4rem 0.8rem",
    borderRadius: "8px",
    fontSize: "0.9rem",
  },
  main: { maxWidth: 800, margin: "0 auto", padding: "2rem" },
  heading: { marginBottom: "1rem" },
  subheading: { margin: "1.5rem 0 0.75rem" },
  muted: { color: "#8888a0" },
  list: { display: "flex", flexDirection: "column", gap: "0.5rem" },
  card: {
    display: "flex",
    alignItems: "center",
    gap: "1rem",
    padding: "1rem",
    background: "#12121a",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    cursor: "pointer",
  },
  hashCode: { fontFamily: "monospace", color: "#00d4aa", fontSize: "0.85rem" },
  localId: {
    fontFamily: "monospace",
    color: "#8888a0",
    fontSize: "0.85rem",
    minWidth: "1.75rem",
    textAlign: "right" as const,
  },
  followUp: {
    fontSize: "0.7rem",
    color: "#8888a0",
    border: "1px solid rgba(136,136,160,0.35)",
    borderRadius: "999px",
    padding: "0.1rem 0.45rem",
  },
  badge: {
    fontSize: "0.75rem",
    padding: "0.15rem 0.5rem",
    borderRadius: "999px",
  },
  date: { marginLeft: "auto", fontSize: "0.8rem", color: "#8888a0" },
  detailCard: {
    background: "#12121a",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    padding: "1.5rem",
    marginBottom: "1rem",
  },
  hashFull: {
    fontFamily: "monospace",
    fontSize: "0.8rem",
    wordBreak: "break-all" as const,
  },
  proof: {
    background: "#0a0a0f",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    padding: "1rem",
    fontFamily: "monospace",
    fontSize: "0.85rem",
    overflowX: "auto",
    whiteSpace: "pre-wrap" as const,
    maxHeight: 400,
    overflowY: "auto",
  },
  publishBtn: {
    background: "#00d4aa",
    color: "#0a0a0f",
    border: "none",
    padding: "0.6rem 1.5rem",
    borderRadius: "8px",
    fontWeight: 600,
    cursor: "pointer",
    marginTop: "1rem",
  },
  message: { marginTop: "0.5rem", fontSize: "0.9rem" },
  noteCard: {
    background: "#12121a",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    padding: "1rem",
    marginBottom: "0.5rem",
  },
  chipSealed: { fontSize: "0.7rem", padding: "0.15rem 0.5rem", borderRadius: 999, border: "1px solid #23233a", color: "#8888a0" },
  chipPublished: { fontSize: "0.7rem", padding: "0.15rem 0.5rem", borderRadius: 999, border: "1px solid #22c55e", color: "#22c55e" },
  chipOk: { fontSize: "0.7rem", padding: "0.15rem 0.5rem", borderRadius: 999, border: "1px solid #00d4aa", color: "#00d4aa" },
  chipWarn: { fontSize: "0.7rem", padding: "0.15rem 0.5rem", borderRadius: 999, border: "1px solid #6b5f2a", color: "#d4b400" },
  chipQuiet: { fontSize: "0.7rem", padding: "0.15rem 0.5rem", borderRadius: 999, border: "1px solid #23233a", color: "#55556a" },
  detailHead: { display: "flex", alignItems: "baseline", gap: "0.75rem", flexWrap: "wrap" as const, marginTop: "0.5rem" },
  detailId: { fontFamily: "monospace", fontSize: "1.4rem", color: "#00d4aa" },
  detailHash: { fontFamily: "monospace", fontSize: "0.85rem", color: "#8888a0", wordBreak: "break-all" as const },
  chipRow: { display: "flex", gap: "0.4rem", flexWrap: "wrap" as const, margin: "0.75rem 0 0" },
  chipBase: { fontSize: "0.7rem", padding: "0.15rem 0.5rem", borderRadius: 999, border: "1px solid #23233a" },
  parentLine: { color: "#8888a0", fontSize: "0.85rem", marginTop: "0.75rem" },
  inlineHash: { fontFamily: "monospace", color: "#8888a0", fontSize: "0.8rem", wordBreak: "break-all" as const },
  section: { marginTop: "2rem", paddingTop: "1.25rem", borderTop: "1px solid #1a1a26" },
  sectionTitle: { fontSize: "1rem", margin: 0 },
  sectionNote: { color: "#8888a0", fontSize: "0.82rem", margin: "0.35rem 0 0.85rem", maxWidth: "70ch", lineHeight: 1.5 },
  byteCount: { color: "#55556a", fontFamily: "monospace", fontSize: "0.75rem", margin: "0.4rem 0 0" },
  secondaryBtn: { marginTop: "0.6rem", padding: "0.5rem 1rem", background: "transparent", color: "#e8e8f0", border: "1px solid #23233a", borderRadius: 6, cursor: "pointer", fontSize: "0.85rem" },
  noteWhen: { color: "#55556a", fontFamily: "monospace", fontSize: "0.75rem", margin: "0 0 0.35rem" },
  noteFilename: { fontSize: "0.75rem", color: "#8888a0", marginBottom: "0.5rem" },
  noteContent: {
    fontFamily: "monospace",
    fontSize: "0.85rem",
    whiteSpace: "pre-wrap" as const,
    margin: 0,
  },
  noteForm: { marginTop: "1rem" },
  textarea: {
    width: "100%",
    padding: "0.7rem",
    background: "#0a0a0f",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    color: "#e0e0e8",
    fontFamily: "monospace",
    fontSize: "0.85rem",
    resize: "vertical" as const,
  },
  addNoteBtn: {
    background: "transparent",
    border: "1px solid #2a2a3a",
    color: "#e0e0e8",
    padding: "0.5rem 1rem",
    borderRadius: "8px",
    cursor: "pointer",
    marginTop: "0.5rem",
  },
  backBtn: {
    background: "none",
    border: "none",
    color: "#00d4aa",
    cursor: "pointer",
    fontSize: "0.9rem",
    marginBottom: "1rem",
    padding: 0,
  },
  label: {
    display: "flex",
    flexDirection: "column",
    gap: "0.3rem",
    fontSize: "0.9rem",
    color: "#8888a0",
    marginBottom: "1rem",
  },
  input: {
    // Inputs sized by content look accidental next to a full-width textarea.
    width: "100%",
    padding: "0.7rem 1rem",
    background: "#0a0a0f",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    color: "#e0e0e8",
    fontSize: "0.95rem",
  },
};
