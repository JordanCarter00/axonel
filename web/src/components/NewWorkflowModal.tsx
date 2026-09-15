import React, { useState } from 'react';
import { api } from '../services/api';
import { X, Sparkles, Play, Layers } from 'lucide-react';

interface NewWorkflowModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated: (workflowId: string) => void;
  workspaceId?: string | null;
  activeWorkspaceName?: string | null;
}

export const NewWorkflowModal: React.FC<NewWorkflowModalProps> = ({
  isOpen,
  onClose,
  onCreated,
  workspaceId,
  activeWorkspaceName,
}) => {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [backend, setBackend] = useState('internal');
  const [autoPlan, setAutoPlan] = useState(true);
  const [autoStart, setAutoStart] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !description.trim()) {
      setError('Please provide both workflow title and objective description.');
      return;
    }

    setIsSubmitting(true);
    setError(null);
    try {
      const created = await api.createWorkflow({
        name: name.trim(),
        description: description.trim(),
        workspace_id: workspaceId || undefined,
        auto_plan: autoPlan,
        auto_start: autoStart,
        backend: backend === 'internal' ? undefined : backend,
      });
      setName('');
      setDescription('');
      setBackend('internal');
      onCreated(created.id);
      onClose();
    } catch (err: unknown) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-surface border border-surface-border rounded-xl max-w-lg w-full shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        {/* Modal Header */}
        <div className="px-6 py-4 border-b border-surface-border flex items-center justify-between bg-[#0e1424]">
          <div className="flex items-center space-x-2.5">
            <div className="p-1.5 bg-primary-600/20 text-indigo-400 rounded-md border border-primary-500/30">
              <Layers className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-slate-100">Create Autonomous Workflow</h2>
              <p className="text-[11px] text-slate-400">Initialize a goal-driven multi-agent task graph</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 text-slate-400 hover:text-slate-200 rounded hover:bg-surface-hover"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Modal Body */}
        <form onSubmit={handleSubmit} noValidate className="p-6 space-y-4">
          {error && (
            <div className="p-3 bg-rose-950/40 border border-rose-500/40 rounded text-xs text-rose-300">
              {error}
            </div>
          )}

          {activeWorkspaceName && (
            <div className="p-2.5 bg-indigo-950/30 border border-indigo-500/30 rounded-lg flex items-center space-x-2 text-xs text-indigo-300">
              <Layers className="w-3.5 h-3.5 shrink-0 text-indigo-400" />
              <span>Workspace: <strong>{activeWorkspaceName}</strong></span>
            </div>
          )}

          <div>
            <label className="block text-xs font-semibold text-slate-300 uppercase tracking-wider mb-1.5">
              Workflow Title
            </label>
            <input
              type="text"
              required
              placeholder="e.g. Implement Token Bucket Rate Limiter"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3.5 py-2 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-300 uppercase tracking-wider mb-1.5">
              Engineering Objective & Scope
            </label>
            <textarea
              required
              rows={4}
              placeholder="Detail the target workload, acceptance criteria, constraints, and verification requirements..."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full bg-[#0a0d14] border border-surface-border rounded-md p-3 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500 font-mono"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-300 uppercase tracking-wider mb-1.5">
              Execution Engine & Agent Backend
            </label>
            <select
              value={backend}
              onChange={(e) => setBackend(e.target.value)}
              className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3 py-2 text-xs text-slate-200 focus:outline-none focus:border-indigo-500 font-mono"
            >
              <option value="internal">Internal Multi-Agent Provider Loop (Default)</option>
              <option value="fake_agent">External Process Host [plexis-fake-agent]</option>
              <option value="claude_code" disabled>Claude Code CLI (Adapter Stub)</option>
              <option value="codex" disabled>Codex CLI (Adapter Stub)</option>
              <option value="gemini_cli" disabled>Gemini CLI (Adapter Stub)</option>
            </select>
          </div>

          <div className="space-y-2.5 pt-2">
            <label className="flex items-center space-x-2.5 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={autoPlan}
                onChange={(e) => setAutoPlan(e.target.checked)}
                className="rounded bg-[#0a0d14] border-surface-border text-indigo-600 focus:ring-indigo-500 w-4 h-4"
              />
              <div className="text-xs">
                <span className="font-semibold text-slate-200 flex items-center space-x-1">
                  <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
                  <span>Autonomous DAG Planning</span>
                </span>
                <span className="text-[11px] text-slate-400 block">
                  Decompose objective into a dependency graph with specialized roles
                </span>
              </div>
            </label>

            <label className="flex items-center space-x-2.5 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={autoStart}
                onChange={(e) => setAutoStart(e.target.checked)}
                className="rounded bg-[#0a0d14] border-surface-border text-indigo-600 focus:ring-indigo-500 w-4 h-4"
              />
              <div className="text-xs">
                <span className="font-semibold text-slate-200 flex items-center space-x-1">
                  <Play className="w-3.5 h-3.5 text-sky-400" />
                  <span>Immediate Execution Dispatch</span>
                </span>
                <span className="text-[11px] text-slate-400 block">
                  Launch runtime lease scheduling immediately after plan formulation
                </span>
              </div>
            </label>
          </div>

          <div className="flex items-center justify-end space-x-2.5 pt-4 border-t border-surface-border">
            <button
              type="button"
              onClick={onClose}
              className="px-3.5 py-2 text-slate-400 hover:text-slate-200 text-xs font-medium"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="px-4 py-2 bg-primary-600 hover:bg-primary-500 disabled:opacity-50 text-white rounded-md text-xs font-semibold shadow-md shadow-indigo-600/20 transition-colors"
            >
              {isSubmitting ? 'Creating Workflow...' : 'Create & Launch'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
