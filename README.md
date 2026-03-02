# Stasher

Stasher is a local-first development history tracker designed to capture the intermediate state of your codebase between Git commits. While Git records what you ship, Stasher records how you built it.

Stasher runs as a background daemon that snapshots every file save, making your entire development history searchable and recoverable without requiring manual commits or cloud synchronization.

---

## Technical Overview

Modern development involves rapid iteration and frequent use of AI-assisted coding tools. During these sessions, multiple implementations are often tried, refined, or overwritten. If a valuable piece of logic is deleted or refactored before a commit is made, it is traditionally lost.

Stasher eliminates this risk by automatically capturing differential snapshots of your workspace. It provides a searchable timeline of your work, allowing you to recover deleted snippets, audit AI-generated refactors, and reconstruct previous states of your logic.

---

## Core Capabilities

### Continuous Snapshotting
Stasher monitors file system events and captures unified diffs on every save. Unlike full-file backups, this approach minimizes storage overhead while maintaining a granular history of every change.

### Content-Addressable Storage (CAS)
Stasher uses a CAS engine for the `.stasher/objects` directory. Every unique version of a file is stored once based on its hash, enabling massive disk space savings through deduplication.

### Smart Move Tracking
History follows your code, not just your filename. If you move a file from `src/old.rs` to `src/new.rs`, Stasher uses content hashes to automatically connect the history, showing you a continuous timeline of the file's evolution across renames.

### Semantic Search and Retrieval
Integrated natural language processing allows you to query your history semantically via `stasher ask`.
- **Natural Language Queries:** Search for concepts like "how I handled the JWT implementation this morning."
- **Deleted Code Recovery:** Surface logic that was removed before a commit.
- **On-Device Privacy:** Uses a local `nomic-embed-text` model. All embeddings are generated and stored 100% locally.

---

## Architecture

Stasher consists of a lightweight daemon and a structured storage layer.

### The Daemon
- **Language:** Built in Rust for minimal resource footprint.
- **Monitoring:** Uses the `notify` crate to watch file saves across your project root.
- **PID Locking**: Ensures only one daemon process can monitor a project at a time, preventing database corruption.
- **Gitignore Awareness**: Automatically respects your `.gitignore` rules (via the `ignore` crate) to keep history clean of `node_modules`, `target`, and other noise.

### Storage Layer
- **Metadata:** SQLite (WAL mode) stores structured records of snapshots, hashes, and session metadata.
- **Vector Search:** LanceDB manages code embeddings for lightning-fast semantic similarity search.
- **CAS Objects**: Content is stored in `.stasher/objects` indexed by BLAKE3 hashes.

---

## Usage

Stasher is controlled via a command-line interface:

- `stasher init`: Initialize a new project and perform an initial sync.
- `stasher daemon`: Start the background watcher (only one instance allowed).
- `stasher show <file>`: View the timeline for a file (including history from moved/renamed versions).
- `stasher diff <snapshot_id>`: Show a colorized diff of exactly what changed in a specific snapshot.
- `stasher ask <query>`: Semantic natural language search across your history.
- `stasher restore <file> --snapshot <id>`: Restore a file. Stasher automatically snapshots your current "unsaved" work before overwriting as a safety net.
- `stasher status`: View project statistics, disk space saved by deduplication, and daemon status.
- `stasher prune --days <n>`: Clean up snapshots older than `n` days and garbage-collect unused objects.

---

## Contributing

Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on branch naming, commit messages, and the development workflow.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
