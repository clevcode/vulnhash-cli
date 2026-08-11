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
  filename: string;
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

export default function App() {
  const [view, setView] = useState<View>("list");
  const [selectedHash, setSelectedHash] = useState("");

  const showDetail = (hash: string) => {
    setSelectedHash(hash);
    setView("detail");
  };

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
          <EntryDetail hash={selectedHash} onBack={() => setView("list")} />
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

  useEffect(() => {
    api.getEntry(hash).then(setEntry);
    api.getProof(hash).then(setProof);
    api.listNotes(hash).then(setNotes);
  }, [hash]);

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
        &larr; Back
      </button>
      <h2 style={styles.heading}>Finding Details</h2>

      <div style={styles.detailCard}>
        <p>
          <strong>ID:</strong> {entry.local_id}
        </p>
        <p>
          <strong>Hash:</strong>{" "}
          <code style={styles.hashFull}>{entry.hash}</code>
        </p>
        <p>
          <strong>Algorithm:</strong> {entry.algorithm}
        </p>
        <p>
          <strong>Submitted:</strong>{" "}
          {new Date(entry.submitted_at).toLocaleString()}
        </p>
        <p>
          <strong>Published:</strong> {entry.published ? "Yes" : "No"}
        </p>
        <p>
          <strong>Synced:</strong> {entry.synced ? "Yes" : "No"}
        </p>
        {entry.parent && (
          <p>
            <strong>Follows up on:</strong>{" "}
            <code style={styles.hashFull}>{entry.parent}</code>
          </p>
        )}
      </div>

      <h3 style={styles.subheading}>Proof Document</h3>
      <pre style={styles.proof}>{proof}</pre>

      {!entry.published && (
        <div style={{ marginBottom: "2rem" }}>
          <button
            style={styles.publishBtn}
            onClick={handlePublish}
            disabled={publishing}
          >
            {publishing ? "Publishing..." : "Publish Finding"}
          </button>
          {message && <p style={styles.message}>{message}</p>}
        </div>
      )}

      <h3 style={styles.subheading}>Notes ({notes.length})</h3>
      {notes.map((n) => (
        <div key={n.filename} style={styles.noteCard}>
          <p style={styles.noteFilename}>{n.filename}</p>
          <pre style={styles.noteContent}>{n.content}</pre>
        </div>
      ))}
      <div style={styles.noteForm}>
        <textarea
          style={styles.textarea}
          placeholder="Add a note..."
          value={newNote}
          onChange={(e) => setNewNote(e.target.value)}
          rows={3}
        />
        <button style={styles.addNoteBtn} onClick={handleAddNote}>
          Add Note
        </button>
      </div>
    </div>
  );
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
    minHeight: "100vh",
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
    padding: "0.7rem 1rem",
    background: "#0a0a0f",
    border: "1px solid #2a2a3a",
    borderRadius: "8px",
    color: "#e0e0e8",
    fontSize: "0.95rem",
  },
};
