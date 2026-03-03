import { useState, useEffect } from 'react';
import axios from 'axios';
import {
  FolderGit2,
  Search,
  History,
  X,
  AlertTriangle,
  CheckCircle2,
  Clock
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

// --- Utils ---
function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// --- Types ---
interface Project {
  id: number;
  name: String;
  path: string;
  last_active: number;
}

interface Snapshot {
  id: string;
  timestamp: number;
  lines_added: number;
  lines_removed: number;
  file_path: string;
  content_hash: string;
  diff_patch: string;
}

interface ProjectStats {
  total_snapshots: number;
  total_size_mb: number;
  dedup_ratio: number;
  pulse: { hour: number, count: number }[];
}

// --- Main App ---
export default function StasherDashboard() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [stats, setStats] = useState<ProjectStats | null>(null);
  const [selectedSnapshot, setSelectedSnapshot] = useState<Snapshot | null>(null);
  const [showDiffModal, setShowDiffModal] = useState(false);

  // Loading States
  const [loadingProjects, setLoadingProjects] = useState(true);
  const [loadingSnapshots, setLoadingSnapshots] = useState(false);

  // Custom Modal States
  const [confirmModal, setConfirmModal] = useState<{ show: boolean, snapshot: Snapshot | null }>({ show: false, snapshot: null });
  const [statusModal, setStatusModal] = useState<{ show: boolean, message: string, type: 'success' | 'error' }>({ show: false, message: '', type: 'success' });

  useEffect(() => {
    fetchProjects();
  }, []);

  const fetchProjects = async () => {
    setLoadingProjects(true);
    try {
      const res = await axios.get('http://localhost:3000/api/projects');
      setProjects(res.data);
      if (res.data.length > 0 && !activeProject) {
        setActiveProject(res.data[0]);
      }
    } catch (err) {
      console.error("Failed to fetch projects", err);
    } finally {
      setLoadingProjects(false);
    }
  };

  const fetchSnapshots = async (project: Project, file?: string, isHidden = false) => {
    if (!isHidden) setLoadingSnapshots(true);
    try {
      const res = await axios.get(`http://localhost:3000/api/snapshots`, {
        params: { project_path: project.path, file_path: file }
      });
      setSnapshots(res.data);
    } catch (err) {
      console.error("Failed to fetch snapshots", err);
    } finally {
      if (!isHidden) setLoadingSnapshots(false);
    }
  };

  const fetchStats = async (project: Project) => {
    try {
      const res = await axios.get(`http://localhost:3000/api/stats`, {
        params: { project_path: project.path }
      });
      setStats(res.data);
    } catch (err) {
      console.error("Failed to fetch stats", err);
    }
  };

  const handleRestore = async (snapshot: Snapshot) => {
    if (!activeProject) return;

    try {
      await axios.post('http://localhost:3000/api/restore', {
        project_path: activeProject.path,
        file_path: snapshot.file_path,
        snapshot_id: snapshot.id
      });
      setConfirmModal({ show: false, snapshot: null });
      setStatusModal({ show: true, message: "File restored successfully!", type: 'success' });

      // Auto-hide success message
      setTimeout(() => setStatusModal(prev => ({ ...prev, show: false })), 3000);
    } catch (err) {
      console.error("Failed to restore", err);
      setStatusModal({ show: true, message: "Restoration failed.", type: 'error' });
    }
  };

  const openDiffModal = (snapshot: Snapshot) => {
    setSelectedSnapshot(snapshot);
    setShowDiffModal(true);
  };

  const triggerConfirm = (snapshot: Snapshot) => {
    setConfirmModal({ show: true, snapshot });
  };

  useEffect(() => {
    if (activeProject) {
      fetchSnapshots(activeProject);
      fetchStats(activeProject);

      // Real-time Heartbeat Polling (Every 3 seconds)
      const interval = setInterval(() => {
        fetchSnapshots(activeProject, undefined, true);
        fetchStats(activeProject);
      }, 3000);

      return () => clearInterval(interval);
    }
  }, [activeProject]);

  return (
    <div className="flex h-screen w-full bg-[#0F172A] text-slate-200 overflow-hidden font-sans">
      {/* Sidebar */}
      <aside className="w-72 glass border-r border-slate-800 flex flex-col p-6 z-10">
        <div className="flex items-center gap-3 mb-10">
          <div className="p-2 rounded-xl bg-brand-teal text-slate-900 shadow-[0_0_20px_rgba(45,212,191,0.3)]">
            <History size={24} />
          </div>
          <h1 className="text-2xl font-bold tracking-tight text-white">Stasher</h1>
        </div>

        <nav className="flex-1 overflow-y-auto space-y-2 no-scrollbar">
          <div className="text-[10px] uppercase tracking-widest text-slate-500 font-bold mb-4 ml-2">Projects</div>
          {loadingProjects ? (
            <div className="space-y-3 px-2">
              {[...Array(3)].map((_, i) => (
                <div key={i} className="h-10 w-full bg-slate-800/30 rounded-xl animate-pulse" />
              ))}
            </div>
          ) : (
            projects.map((proj) => (
              <button
                key={proj.id}
                onClick={() => setActiveProject(proj)}
                className={cn(
                  "w-full flex items-center justify-between p-3 rounded-xl transition-all duration-200 group relative",
                  activeProject?.id === proj.id
                    ? "bg-slate-800/50 text-white border-l-2 border-brand-teal shadow-lg"
                    : "text-slate-400 hover:bg-slate-800/20 hover:text-slate-200"
                )}
              >
                <div className="flex items-center gap-3">
                  <FolderGit2 size={18} className={cn(activeProject?.id === proj.id ? "text-brand-teal" : "text-slate-500 group-hover:text-slate-400")} />
                  <span className="text-sm font-medium">{proj.name}</span>
                </div>
                <div className={cn(
                  "w-1.5 h-1.5 rounded-full",
                  activeProject?.id === proj.id ? "bg-brand-teal animate-pulse" : "bg-slate-700"
                )} />
              </button>
            ))
          )}
        </nav>

      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col relative overflow-hidden">
        {/* Header */}
        <header className="h-20 glass border-b border-slate-800 flex items-center justify-between px-8 z-10">
          <div className="relative w-[480px]">
            <Search className="absolute left-4 top-1/2 -translate-y-1/2 text-slate-500" size={18} />
            <input
              type="text"
              placeholder="Search history across all projects..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-slate-900/50 border border-slate-700 rounded-2xl py-2.5 pl-11 pr-4 text-sm focus:outline-none focus:ring-2 focus:ring-brand-teal/50 focus:border-brand-teal/50 transition-all text-slate-200 placeholder:text-slate-600"
            />
          </div>

        </header>

        {/* Content Area */}
        <div className="flex-1 flex overflow-hidden">
          {/* History Timeline */}
          <section className="flex-1 p-8 overflow-y-auto custom-scrollbar">
            <div className="flex items-center justify-between mb-8">
              <div>
                <h2 className="text-3xl font-bold text-white mb-1">Time Machine</h2>
                <div className="flex items-center gap-2">
                  <p className="text-slate-500 text-sm">Showing history for <span className="text-brand-teal font-medium">{activeProject?.name}</span></p>
                  <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-brand-teal/10 border border-brand-teal/20 ml-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-brand-teal animate-pulse" />
                    <span className="text-[10px] font-bold text-brand-teal uppercase tracking-wider">Live</span>
                  </div>
                </div>
              </div>
            </div>

            <div className="relative pl-10 space-y-12 min-h-[400px]">
              {loadingSnapshots ? (
                <div className="space-y-12">
                  {[...Array(2)].map((_, i) => (
                    <div key={i} className="animate-pulse relative">
                      <div className="absolute -left-[10px] top-4 w-5 h-5 rounded-full border-4 border-[#0F172A] bg-slate-800 z-20" />
                      <div className="glass rounded-3xl p-6 border border-slate-800/50 h-64" />
                    </div>
                  ))}
                </div>
              ) : snapshots.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-20 bg-slate-900/20 rounded-3xl border border-dashed border-slate-800">
                  <div className="p-4 rounded-full bg-slate-800/50 mb-4">
                    <History size={32} className="text-slate-500" />
                  </div>
                  <h3 className="text-lg font-bold text-white">No history yet</h3>
                  <p className="text-sm text-slate-500 mt-1 max-w-xs text-center">Save a file to start tracking hisory for <span className="text-brand-teal">{activeProject?.name}</span></p>
                </div>
              ) : (
                <>
                  {/* Vertical Line (The DNA Strand) */}
                  <div className="absolute left-[19px] top-4 bottom-4 w-0.5 bg-gradient-to-b from-brand-teal via-slate-800 to-slate-800 rounded-full" />
                  <AnimatePresence>
                    {snapshots.map((snap, i) => (
                      <motion.div
                        key={snap.id}
                        initial={{ opacity: 0, x: -20 }}
                        animate={{ opacity: 1, x: 0 }}
                        transition={{ delay: i * 0.05 }}
                        className="relative"
                      >
                        {/* Circle Indicator */}
                        <div className="absolute -left-[10px] top-4 w-5 h-5 rounded-full border-4 border-[#0F172A] bg-brand-teal shadow-[0_0_10px_rgba(45,212,191,0.5)] z-20" />

                        <div className="glass rounded-3xl p-6 hover:border-slate-600/50 transition-all duration-300 group">
                          <div className="flex items-center justify-between mb-6">
                            <div className="flex items-center gap-3">
                              <code className="text-[10px] bg-slate-800/80 px-2 py-1 rounded text-slate-300 font-bold">SHA: {snap.id.slice(0, 7)}</code>
                              <span className="text-xs text-slate-500 flex items-center gap-1.5">
                                <Clock size={12} /> {new Date(snap.timestamp).toLocaleString()}
                              </span>
                            </div>
                            <div className="flex items-center gap-2">
                              <span className="px-2 py-1 rounded-lg bg-emerald-500/10 text-emerald-400 text-[10px] font-bold">+{snap.lines_added}</span>
                              <span className="px-2 py-1 rounded-lg bg-rose-500/10 text-rose-400 text-[10px] font-bold">-{snap.lines_removed}</span>
                            </div>
                          </div>

                          <div className="mb-4">
                            <div className="text-sm text-slate-300 font-bold mb-1">Impacted File: {snap.file_path}</div>
                            <p className="text-xs text-slate-500 leading-relaxed">Recorded during development session on file save trigger.</p>
                          </div>

                          <div className="bg-[#0b0f19] rounded-2xl p-4 border border-slate-800/50 max-h-60 overflow-hidden relative">
                            <pre className="text-[11px] leading-6 font-mono text-slate-400 overflow-x-auto whitespace-pre">
                              {snap.diff_patch || "No diff record available."}
                            </pre>
                            <div className="absolute bottom-0 left-0 right-0 h-10 bg-gradient-to-t from-[#0b0f19] to-transparent" />
                          </div>

                          <div className="mt-4 flex gap-3 opacity-0 group-hover:opacity-100 transition-all transform translate-y-2 group-hover:translate-y-0">
                            <button
                              onClick={() => triggerConfirm(snap)}
                              className="px-4 py-1.5 rounded-lg bg-brand-teal text-slate-900 text-[11px] font-bold hover:scale-[1.02] active:scale-[0.98] transition-all"
                            >
                              Restore Version
                            </button>
                            <button
                              onClick={() => openDiffModal(snap)}
                              className="px-4 py-1.5 rounded-lg bg-slate-800 text-slate-300 text-[11px] font-bold hover:bg-slate-700 transition-all"
                            >
                              View Full Diff
                            </button>
                          </div>
                        </div>
                      </motion.div>
                    ))}
                  </AnimatePresence>
                </>
              )}
            </div>
          </section>

          {/* Right Stats Sidebar */}
          <aside className="w-80 p-8 glass border-l border-slate-800 flex flex-col gap-8">
            <div className="space-y-4">
              <h3 className="text-xs font-bold text-slate-500 tracking-widest uppercase mb-4">Project Stats</h3>
              <div className="grid grid-cols-2 gap-3">
                <div className="p-4 rounded-2xl bg-slate-900/50 border border-slate-800">
                  <div className="text-brand-teal font-bold text-xl mb-1">{stats?.total_snapshots || 0}</div>
                  <div className="text-[9px] text-slate-500 font-bold uppercase tracking-wider">Snapshots</div>
                </div>
                <div className="p-4 rounded-2xl bg-slate-900/50 border border-slate-800">
                  <div className="text-brand-coral font-bold text-xl mb-1">{stats?.total_size_mb.toFixed(2) || "0.00"}MB</div>
                  <div className="text-[9px] text-slate-500 font-bold uppercase tracking-wider">Storage</div>
                </div>
              </div>
            </div>

            <div className="space-y-4">
              <h3 className="text-xs font-bold text-slate-500 tracking-widest uppercase">Productivity Pulse</h3>
              <div className="h-40 flex items-end gap-[3px]">
                {(stats?.pulse || [...Array(24)].map((_, i) => ({ hour: i, count: 0 }))).map((p, i) => (
                  <div
                    key={i}
                    className="flex-1 rounded-full bg-brand-teal/20 hover:bg-brand-teal transition-all cursor-pointer"
                    style={{ height: `${Math.min(100, (p.count / 5) * 100 + 5)}%` }}
                    title={`Hour ${p.hour}: ${p.count} snapshots`}
                  />
                ))}
              </div>
              <div className="flex justify-between text-[8px] font-bold text-slate-600 uppercase tracking-widest px-1">
                <span>00:00</span>
                <span>12:00</span>
                <span>23:59</span>
              </div>
            </div>

          </aside>
        </div>
      </main>

      {/* Global Glow Backgrounds */}
      <div className="fixed top-[-10%] right-[-5%] w-[40%] h-[40%] bg-brand-teal/5 blur-[120px] rounded-full pointer-events-none" />
      <div className="fixed bottom-[-10%] left-[-5%] w-[40%] h-[40%] bg-brand-coral/5 blur-[120px] rounded-full pointer-events-none" />

      {/* Diff Modal */}
      <AnimatePresence>
        {showDiffModal && selectedSnapshot && (
          <div className="fixed inset-0 z-50 flex items-center justify-center p-6 bg-slate-950/80 backdrop-blur-sm">
            <motion.div
              initial={{ opacity: 0, scale: 0.95, y: 20 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.95, y: 20 }}
              className="bg-[#0F172A] border border-slate-800 w-full max-w-4xl max-h-[80vh] rounded-3xl overflow-hidden flex flex-col shadow-2xl"
            >
              <div className="p-6 border-b border-slate-800 flex items-center justify-between">
                <div>
                  <h3 className="text-xl font-bold text-white">Snapshot Diff</h3>
                  <p className="text-xs text-slate-500 mt-1">{selectedSnapshot.file_path} • {new Date(selectedSnapshot.timestamp).toLocaleString()}</p>
                </div>
                <button
                  onClick={() => setShowDiffModal(false)}
                  className="p-2 rounded-xl hover:bg-slate-800 text-slate-400 transition-all"
                >
                  <X size={20} />
                </button>
              </div>
              <div className="flex-1 overflow-auto p-6 bg-[#0b0f19] custom-scrollbar">
                <pre className="text-xs font-mono leading-relaxed text-slate-300 whitespace-pre">
                  {selectedSnapshot.diff_patch.split('\n').map((line, i) => (
                    <div
                      key={i}
                      className={cn(
                        "px-2 py-0.5 rounded",
                        line.startsWith('+') ? "bg-emerald-500/10 text-emerald-400" :
                          line.startsWith('-') ? "bg-rose-500/10 text-rose-400" :
                            line.startsWith('@@') ? "text-brand-teal opacity-50" : ""
                      )}
                    >
                      {line}
                    </div>
                  ))}
                </pre>
              </div>
              <div className="p-6 border-t border-slate-800 flex justify-end gap-3 bg-slate-900/20">
                <button
                  onClick={() => setShowDiffModal(false)}
                  className="px-6 py-2 rounded-xl text-sm font-bold text-slate-400 hover:text-white transition-all"
                >
                  Close
                </button>
                <button
                  onClick={() => {
                    triggerConfirm(selectedSnapshot);
                    setShowDiffModal(false);
                  }}
                  className="px-6 py-2 rounded-xl bg-brand-teal text-slate-900 text-sm font-bold hover:scale-[1.02] active:scale-[0.98] transition-all"
                >
                  Restore This Version
                </button>
              </div>
            </motion.div>
          </div>
        )}
      </AnimatePresence>

      {/* Custom Confirmation Modal */}
      <AnimatePresence>
        {confirmModal.show && confirmModal.snapshot && (
          <div className="fixed inset-0 z-[60] flex items-center justify-center p-6 bg-slate-950/40 backdrop-blur-md">
            <motion.div
              initial={{ opacity: 0, scale: 0.9, y: 20 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.9, y: 20 }}
              className="bg-[#1e293b] border border-slate-700 w-full max-w-md rounded-3xl p-8 shadow-2xl text-center"
            >
              <div className="w-16 h-16 rounded-full bg-brand-coral/10 flex items-center justify-center text-brand-coral mx-auto mb-6">
                <AlertTriangle size={32} />
              </div>
              <h3 className="text-xl font-bold text-white mb-2">Confirm Restoration</h3>
              <p className="text-sm text-slate-400 mb-8 leading-relaxed">
                Are you sure you want to restore <span className="text-white font-medium">{confirmModal.snapshot.file_path}</span>?
                This will overwrite your current file with the version from <span className="text-brand-teal">{new Date(confirmModal.snapshot.timestamp).toLocaleTimeString()}</span>.
              </p>
              <div className="flex gap-3">
                <button
                  onClick={() => setConfirmModal({ show: false, snapshot: null })}
                  className="flex-1 px-6 py-3 rounded-xl bg-slate-800 text-slate-300 font-bold text-sm hover:bg-slate-700 transition-all"
                >
                  Cancel
                </button>
                <button
                  onClick={() => handleRestore(confirmModal.snapshot!)}
                  className="flex-1 px-6 py-3 rounded-xl bg-brand-teal text-slate-900 font-bold text-sm hover:scale-[1.02] active:scale-[0.98] transition-all shadow-[0_0_20px_rgba(45,212,191,0.2)]"
                >
                  Yes, Restore
                </button>
              </div>
            </motion.div>
          </div>
        )}
      </AnimatePresence>

      {/* Custom Status Toast */}
      <AnimatePresence>
        {statusModal.show && (
          <motion.div
            initial={{ opacity: 0, y: 50, scale: 0.9 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 50, scale: 0.9 }}
            className={cn(
              "fixed bottom-8 left-1/2 -translate-x-1/2 z-[70] px-6 py-4 rounded-2xl shadow-2xl border flex items-center gap-3",
              statusModal.type === 'success' ? "bg-emerald-950/80 border-emerald-500/30 text-emerald-400" : "bg-rose-950/80 border-rose-500/30 text-rose-400"
            )}
          >
            {statusModal.type === 'success' ? <CheckCircle2 size={20} /> : <AlertTriangle size={20} />}
            <span className="text-sm font-bold">{statusModal.message}</span>
            <button onClick={() => setStatusModal(prev => ({ ...prev, show: false }))} className="ml-4 opacity-50 hover:opacity-100">
              <X size={16} />
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
