import { useState, useEffect } from 'react';
import axios from 'axios';
import {
  FolderGit2,
  Search,
  History,
  Zap,
  Settings,
  Bell,
  Command,
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
}

// --- Main App ---
export default function StasherDashboard() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [searchQuery, setSearchQuery] = useState('');

  useEffect(() => {
    fetchProjects();
  }, []);

  const fetchProjects = async () => {
    try {
      const res = await axios.get('http://localhost:3000/api/projects');
      setProjects(res.data);
      if (res.data.length > 0 && !activeProject) {
        setActiveProject(res.data[0]);
      }
    } catch (err) {
      console.error("Failed to fetch projects", err);
    }
  };

  const fetchSnapshots = async (project: Project, file = "src/main.rs") => {
    try {
      const res = await axios.get(`http://localhost:3000/api/snapshots`, {
        params: { project_path: project.path, file_path: file }
      });
      setSnapshots(res.data);
    } catch (err) {
      console.error("Failed to fetch snapshots", err);
    }
  };

  useEffect(() => {
    if (activeProject) {
      fetchSnapshots(activeProject);
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
          {projects.map((proj) => (
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
          ))}
        </nav>

        <div className="mt-auto space-y-4 pt-6 border-t border-slate-800">
          <button className="flex items-center gap-3 p-2 text-slate-400 hover:text-white transition-colors">
            <Settings size={20} />
            <span className="text-sm">Settings</span>
          </button>
          <div className="flex items-center gap-3 p-2">
            <div className="w-8 h-8 rounded-lg bg-brand-coral/20 border border-brand-coral/30 flex items-center justify-center text-[10px] font-bold text-brand-coral">
              ST
            </div>
            <div className="flex flex-col">
              <span className="text-xs font-bold text-slate-200">Rahul Roots</span>
              <span className="text-[10px] text-slate-500">Pro Developer</span>
            </div>
          </div>
        </div>
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
            <div className="absolute right-3 top-1/2 -translate-y-1/2 px-2 py-0.5 rounded border border-slate-700 bg-slate-800 text-[10px] text-slate-500 flex items-center gap-1">
              <Command size={10} /> K
            </div>
          </div>

          <div className="flex items-center gap-4">
            <button className="p-2.5 rounded-xl bg-slate-900 border border-slate-800 text-slate-400 hover:text-white transition-all">
              <Bell size={20} />
            </button>
            <button className="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-brand-teal text-slate-900 font-bold text-sm shadow-[0_0_20px_rgba(45,212,191,0.2)] hover:scale-[1.02] active:scale-[0.98] transition-all">
              <Zap size={16} /> Live View
            </button>
          </div>
        </header>

        {/* Content Area */}
        <div className="flex-1 flex overflow-hidden">
          {/* History Timeline */}
          <section className="flex-1 p-8 overflow-y-auto custom-scrollbar">
            <div className="flex items-center justify-between mb-8">
              <div>
                <h2 className="text-3xl font-bold text-white mb-1">Time Machine</h2>
                <p className="text-slate-500 text-sm">Showing history for <span className="text-brand-teal font-medium">{activeProject?.name}</span></p>
              </div>
              <div className="flex gap-2">
                <button className="px-3 py-1.5 rounded-lg bg-slate-800 border border-slate-700 text-[11px] font-bold text-slate-300 hover:bg-slate-700 transition-all">Today</button>
                <button className="px-3 py-1.5 rounded-lg bg-slate-900 border border-slate-800 text-[11px] font-bold text-slate-500 hover:text-slate-400 transition-all">Filters</button>
              </div>
            </div>

            <div className="relative pl-10 space-y-12">
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

                      <div className="bg-[#0b0f19] rounded-2xl p-4 border border-slate-800/50">
                        <pre className="text-[11px] leading-6 font-mono text-slate-400 overflow-x-auto whitespace-pre">
                          {`function update_logic(delta) {
  const result = process(delta);
  // Snapshot captured here
  return validate(result);
}`}
                        </pre>
                      </div>

                      <div className="mt-4 flex gap-3 opacity-0 group-hover:opacity-100 transition-all transform translate-y-2 group-hover:translate-y-0">
                        <button className="px-4 py-1.5 rounded-lg bg-brand-teal text-slate-900 text-[11px] font-bold hover:scale-[1.02] active:scale-[0.98] transition-all">Restore Version</button>
                        <button className="px-4 py-1.5 rounded-lg bg-slate-800 text-slate-300 text-[11px] font-bold hover:bg-slate-700 transition-all">View Full Diff</button>
                      </div>
                    </div>
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>
          </section>

          {/* Right Stats Sidebar */}
          <aside className="w-80 p-8 glass border-l border-slate-800 flex flex-col gap-8">
            <div className="space-y-4">
              <h3 className="text-xs font-bold text-slate-500 tracking-widest uppercase mb-4">Project Stats</h3>
              <div className="grid grid-cols-2 gap-3">
                <div className="p-4 rounded-2xl bg-slate-900/50 border border-slate-800">
                  <div className="text-brand-teal font-bold text-xl mb-1">{snapshots.length}</div>
                  <div className="text-[9px] text-slate-500 font-bold uppercase tracking-wider">Snapshots</div>
                </div>
                <div className="p-4 rounded-2xl bg-slate-900/50 border border-slate-800">
                  <div className="text-brand-coral font-bold text-xl mb-1">0.12MB</div>
                  <div className="text-[9px] text-slate-500 font-bold uppercase tracking-wider">Storage</div>
                </div>
              </div>
            </div>

            <div className="space-y-4">
              <h3 className="text-xs font-bold text-slate-500 tracking-widest uppercase">Productivity Pulse</h3>
              <div className="h-40 flex items-end gap-[3px]">
                {[...Array(24)].map((_, i) => (
                  <div
                    key={i}
                    className="flex-1 rounded-full bg-brand-teal/20 hover:bg-brand-teal transition-all cursor-pointer"
                    style={{ height: `${Math.random() * 100}%` }}
                    title={`Hour ${i}: ${Math.floor(Math.random() * 20)} snapshots`}
                  />
                ))}
              </div>
              <div className="flex justify-between text-[8px] font-bold text-slate-600 uppercase tracking-widest px-1">
                <span>00:00</span>
                <span>12:00</span>
                <span>23:59</span>
              </div>
            </div>

            <div className="p-6 rounded-3xl bg-brand-teal/5 border border-brand-teal/10 mt-auto">
              <div className="flex items-center gap-3 mb-3">
                <Zap size={18} className="text-brand-teal" />
                <span className="text-xs font-bold text-brand-teal">Deduplication Savings</span>
              </div>
              <div className="text-2xl font-bold text-white mb-1">84%</div>
              <p className="text-[10px] text-slate-500 leading-relaxed">Your history is highly compressed using CAS (Content Addressable Storage).</p>
            </div>
          </aside>
        </div>
      </main>

      {/* Global Glow Backgrounds */}
      <div className="fixed top-[-10%] right-[-5%] w-[40%] h-[40%] bg-brand-teal/5 blur-[120px] rounded-full pointer-events-none" />
      <div className="fixed bottom-[-10%] left-[-5%] w-[40%] h-[40%] bg-brand-coral/5 blur-[120px] rounded-full pointer-events-none" />
    </div>
  );
}
