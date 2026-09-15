import React, { useState, useEffect } from 'react';
import { Workspace } from '../types';
import { api } from '../services/api';

interface WorkspaceModalProps {
  isOpen: boolean;
  onClose: () => void;
  activeWorkspaceId?: string | null;
  onSelectWorkspace: (workspace: Workspace) => void;
}

export const WorkspaceModal: React.FC<WorkspaceModalProps> = ({
  isOpen,
  onClose,
  activeWorkspaceId,
  onSelectWorkspace,
}) => {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  // New Workspace form state
  const [showCreateForm, setShowCreateForm] = useState<boolean>(false);
  const [newName, setNewName] = useState('');
  const [newPath, setNewPath] = useState('');
  const [newDescription, setNewDescription] = useState('');
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (isOpen) {
      loadWorkspaces();
    }
  }, [isOpen]);

  const loadWorkspaces = async () => {
    try {
      setLoading(true);
      setError(null);
      const list = await api.listWorkspaces();
      setWorkspaces(list);
    } catch (err: any) {
      setError(err.message || 'Failed to load workspaces');
    } finally {
      setLoading(false);
    }
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newPath.trim()) return;

    try {
      setCreating(true);
      setError(null);
      const created = await api.createWorkspace({
        name: newName.trim(),
        canonical_path: newPath.trim(),
        description: newDescription.trim() || undefined,
        is_default: workspaces.length === 0,
      });
      setWorkspaces((prev) => [...prev, created]);
      onSelectWorkspace(created);
      setShowCreateForm(false);
      setNewName('');
      setNewPath('');
      setNewDescription('');
    } catch (err: any) {
      setError(err.message || 'Failed to create workspace');
    } finally {
      setCreating(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="bg-slate-900 border border-slate-700 rounded-xl shadow-2xl w-full max-w-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-950/50">
          <div className="flex items-center space-x-2">
            <svg className="w-5 h-5 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            <h2 className="text-lg font-semibold text-slate-100">Project Workspaces</h2>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 p-1 rounded-lg hover:bg-slate-800 transition"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Content */}
        <div className="p-6 overflow-y-auto space-y-4 flex-1">
          {error && (
            <div className="p-3 bg-red-950/50 border border-red-800 rounded-lg text-red-300 text-sm">
              {error}
            </div>
          )}

          {!showCreateForm ? (
            <>
              <div className="flex justify-between items-center mb-2">
                <span className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
                  Available Workspaces ({workspaces.length})
                </span>
                <button
                  onClick={() => setShowCreateForm(true)}
                  className="px-3 py-1.5 text-xs font-medium bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition flex items-center space-x-1 shadow-sm"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M12 4v16m8-8H4" />
                  </svg>
                  <span>Register Workspace</span>
                </button>
              </div>

              {loading ? (
                <div className="py-8 text-center text-slate-400">Loading workspaces...</div>
              ) : workspaces.length === 0 ? (
                <div className="py-8 text-center bg-slate-950/40 rounded-xl border border-dashed border-slate-800 p-6">
                  <p className="text-slate-400 text-sm mb-3">No workspaces registered yet.</p>
                  <button
                    onClick={() => setShowCreateForm(true)}
                    className="px-4 py-2 text-sm bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition font-medium"
                  >
                    Register First Workspace
                  </button>
                </div>
              ) : (
                <div className="space-y-2">
                  {workspaces.map((ws) => {
                    const isActive = ws.id === activeWorkspaceId;
                    const branch = ws.vcs?.branch;
                    const headSha = ws.vcs?.head_sha ? ws.vcs.head_sha.substring(0, 7) : null;
                    const isDirty = ws.vcs?.is_dirty;

                    return (
                      <div
                        key={ws.id}
                        onClick={() => {
                          onSelectWorkspace(ws);
                          onClose();
                        }}
                        className={`p-4 rounded-xl border transition cursor-pointer flex items-center justify-between ${
                          isActive
                            ? 'bg-indigo-950/30 border-indigo-500/50 shadow-md ring-1 ring-indigo-500/30'
                            : 'bg-slate-800/40 border-slate-800 hover:bg-slate-800/70 hover:border-slate-700'
                        }`}
                      >
                        <div className="min-w-0 flex-1 pr-4">
                          <div className="flex items-center space-x-2">
                            <span className="font-semibold text-slate-200 truncate">{ws.name}</span>
                            {isActive && (
                              <span className="px-2 py-0.5 text-xs bg-indigo-500/20 text-indigo-300 font-medium rounded-full border border-indigo-500/30">
                                Active
                              </span>
                            )}
                            {branch && (
                              <span className="px-2 py-0.5 text-xs bg-slate-800 text-slate-300 font-mono rounded flex items-center space-x-1 border border-slate-700">
                                <svg className="w-3 h-3 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
                                </svg>
                                <span>{branch}</span>
                                {headSha && <span className="text-slate-500">@{headSha}</span>}
                                {isDirty && <span className="text-amber-400 font-bold" title="Uncommitted changes">*</span>}
                              </span>
                            )}
                          </div>
                          <p className="text-xs text-slate-400 font-mono truncate mt-1">{ws.canonical_path}</p>
                        </div>
                        <div className="text-slate-400 text-xs">
                          {isActive ? (
                            <span className="text-indigo-400 font-medium flex items-center space-x-1">
                              <span>Selected</span>
                              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M5 13l4 4L19 7" />
                              </svg>
                            </span>
                          ) : (
                            <span className="hover:text-slate-200">Select</span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </>
          ) : (
            <form onSubmit={handleCreate} className="space-y-4">
              <div className="flex justify-between items-center pb-2 border-b border-slate-800">
                <span className="font-semibold text-slate-200 text-sm">Register New Project Workspace</span>
                <button
                  type="button"
                  onClick={() => setShowCreateForm(false)}
                  className="text-xs text-slate-400 hover:text-slate-200"
                >
                  Cancel
                </button>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-300 mb-1">
                  Workspace Name <span className="text-red-400">*</span>
                </label>
                <input
                  type="text"
                  required
                  placeholder="e.g. token-limiter"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-300 mb-1">
                  Absolute Canonical Path <span className="text-red-400">*</span>
                </label>
                <input
                  type="text"
                  required
                  placeholder="e.g. /home/user/Projects/token-limiter"
                  value={newPath}
                  onChange={(e) => setNewPath(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm font-mono text-slate-200 focus:outline-none focus:border-indigo-500"
                />
                <p className="text-xs text-slate-500 mt-1">
                  Plexis strictly confines file reads, writes, and shell execution inside this directory root.
                </p>
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-300 mb-1">Description</label>
                <input
                  type="text"
                  placeholder="Optional workspace description"
                  value={newDescription}
                  onChange={(e) => setNewDescription(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div className="pt-2 flex justify-end space-x-2">
                <button
                  type="button"
                  onClick={() => setShowCreateForm(false)}
                  className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={creating || !newName.trim() || !newPath.trim()}
                  className="px-4 py-2 text-sm bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg font-medium transition shadow-sm"
                >
                  {creating ? 'Registering...' : 'Register Workspace'}
                </button>
              </div>
            </form>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-3 border-t border-slate-800 bg-slate-950/50 flex justify-between items-center text-xs text-slate-500">
          <span>Security: Path confinement and secret redactor enabled</span>
          <button
            onClick={onClose}
            className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg transition"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
};
