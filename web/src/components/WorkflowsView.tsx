import React, { useState } from 'react';
import { Workflow } from '../types';
import { Layers, Plus, Search, ArrowRight } from 'lucide-react';

interface WorkflowsViewProps {
  workflows: Workflow[];
  loading: boolean;
  onSelectWorkflow: (id: string) => void;
  onOpenNewWorkflow: () => void;
}

export const WorkflowsView: React.FC<WorkflowsViewProps> = ({
  workflows,
  loading,
  onSelectWorkflow,
  onOpenNewWorkflow,
}) => {
  const [searchTerm, setSearchTerm] = useState('');
  const [filterState, setFilterState] = useState<string>('ALL');

  const filteredWorkflows = workflows.filter((w) => {
    const title = w.name || w.title || '';
    const desc = w.description || w.objective || '';
    const matchesSearch =
      title.toLowerCase().includes(searchTerm.toLowerCase()) ||
      desc.toLowerCase().includes(searchTerm.toLowerCase()) ||
      w.id.toLowerCase().includes(searchTerm.toLowerCase());
    const matchesFilter =
      filterState === 'ALL' || w.state.toUpperCase() === filterState.toUpperCase();
    return matchesSearch && matchesFilter;
  });

  const getStatusColor = (state: string) => {
    switch (state.toLowerCase()) {
      case 'executing':
        return 'bg-sky-500/10 text-sky-400 border-sky-500/30 animate-pulse';
      case 'completed':
        return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      case 'planned':
        return 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
      case 'failed':
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      case 'paused':
        return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
      default:
        return 'bg-slate-800 text-slate-400 border-slate-700';
    }
  };

  return (
    <div className="space-y-6 pb-12">
      {/* Top action row */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Layers className="w-5 h-5 text-indigo-400" />
            <span>Workflow Orchestration</span>
          </h1>
          <p className="text-xs text-slate-400">
            Autonomous multi-agent task graphs and lifecycle state
          </p>
        </div>

        <button
          onClick={onOpenNewWorkflow}
          className="flex items-center space-x-1.5 px-3.5 py-2 bg-primary-600 hover:bg-primary-500 text-white rounded-md text-xs font-semibold shadow-md shadow-indigo-600/20 transition-colors"
        >
          <Plus className="w-4 h-4" />
          <span>New Workflow</span>
        </button>
      </div>

      {/* Filter and Search Bar */}
      <div className="flex flex-col sm:flex-row items-center justify-between gap-3 bg-surface border border-surface-border p-3 rounded-lg">
        <div className="relative w-full sm:w-72">
          <Search className="w-4 h-4 text-slate-400 absolute left-3 top-2.5" />
          <input
            type="text"
            placeholder="Search workflows..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-[#0a0d14] border border-surface-border rounded-md pl-9 pr-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
          />
        </div>

        <div className="flex items-center space-x-1 overflow-x-auto w-full sm:w-auto">
          {['ALL', 'EXECUTING', 'PLANNED', 'COMPLETED', 'PAUSED', 'FAILED'].map((st) => (
            <button
              key={st}
              onClick={() => setFilterState(st)}
              className={`px-2.5 py-1 rounded text-xs font-mono transition-colors ${
                filterState === st
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {st}
            </button>
          ))}
        </div>
      </div>

      {/* Workflows List Table */}
      <div className="bg-surface border border-surface-border rounded-lg overflow-hidden divide-y divide-surface-border">
        {loading && workflows.length === 0 ? (
          <div className="p-12 text-center text-slate-400 font-mono text-xs">
            Loading workflows...
          </div>
        ) : filteredWorkflows.length === 0 ? (
          <div className="p-12 text-center text-slate-400">
            <p className="text-sm">No workflows match the current filter.</p>
          </div>
        ) : (
          filteredWorkflows.map((w) => (
            <div
              key={w.id}
              onClick={() => onSelectWorkflow(w.id)}
              className="p-4 hover:bg-surface-hover cursor-pointer transition-colors flex items-center justify-between"
            >
              <div className="space-y-1.5 flex-1 pr-4">
                <div className="flex items-center space-x-3">
                  <span className="font-semibold text-slate-100 text-sm">{w.name || w.title}</span>
                  <span
                    className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${getStatusColor(
                      w.state
                    )}`}
                  >
                    {w.state}
                  </span>
                </div>
                <p className="text-xs text-slate-400 line-clamp-1">{w.description || w.objective}</p>
                <div className="text-[10px] font-mono text-slate-500 flex items-center space-x-3">
                  <span>ID: {w.id}</span>
                  <span>•</span>
                  <span>Created: {new Date(w.created_at).toLocaleString()}</span>
                </div>
              </div>
              <div className="flex items-center space-x-2 text-slate-400">
                <span className="text-xs font-medium text-primary-400">Inspect</span>
                <ArrowRight className="w-4 h-4 text-slate-500" />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
