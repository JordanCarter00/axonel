import React, { useEffect, useState } from 'react';
import { MemoryRecord } from '../types';
import { api } from '../services/api';
import { Database, Search, Filter, RefreshCw } from 'lucide-react';

export const MemoryView: React.FC = () => {
  const [memories, setMemories] = useState<MemoryRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [scopeFilter, setScopeFilter] = useState<string>('ALL');
  const [search, setSearch] = useState('');

  const loadMemories = async () => {
    setLoading(true);
    try {
      const scope = scopeFilter === 'ALL' ? undefined : scopeFilter;
      const data = await api.listMemories(scope);
      setMemories(data);
    } catch (e) {
      console.error('Failed to load memories:', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadMemories();
  }, [scopeFilter]);

  const filtered = memories.filter((m) => {
    const s = search.toLowerCase();
    return (
      m.key.toLowerCase().includes(s) ||
      m.content.toLowerCase().includes(s) ||
      m.scope_id.toLowerCase().includes(s)
    );
  });

  return (
    <div className="space-y-6 pb-12">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Database className="w-5 h-5 text-indigo-400" />
            <span>Durable Memory Store</span>
          </h1>
          <p className="text-xs text-slate-400">
            Hierarchical memory subsystem with Global, Workflow, Agent, and Task scopes
          </p>
        </div>
        <button
          onClick={loadMemories}
          className="p-2 text-slate-400 hover:text-slate-200 bg-surface border border-surface-border rounded-md"
        >
          <RefreshCw className="w-4 h-4" />
        </button>
      </div>

      {/* Filter and Search */}
      <div className="flex flex-col sm:flex-row items-center justify-between gap-3 bg-surface border border-surface-border p-3 rounded-lg">
        <div className="relative w-full sm:w-72">
          <Search className="w-4 h-4 text-slate-400 absolute left-3 top-2.5" />
          <input
            type="text"
            placeholder="Search keys or memory content..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full bg-[#0a0d14] border border-surface-border rounded-md pl-9 pr-3 py-1.5 text-xs text-slate-200"
          />
        </div>

        <div className="flex items-center space-x-2">
          <Filter className="w-3.5 h-3.5 text-slate-400" />
          {['ALL', 'Global', 'Workflow', 'Agent', 'Task'].map((sc) => (
            <button
              key={sc}
              onClick={() => setScopeFilter(sc)}
              className={`px-2.5 py-1 rounded text-xs font-mono transition-colors ${
                scopeFilter === sc
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {sc}
            </button>
          ))}
        </div>
      </div>

      {/* Memory Records List */}
      <div className="space-y-3">
        {loading && memories.length === 0 ? (
          <div className="p-12 text-center text-slate-400 font-mono text-xs">
            Loading memories...
          </div>
        ) : filtered.length === 0 ? (
          <div className="bg-surface border border-surface-border rounded-lg p-12 text-center text-slate-400">
            No memories found matching filter.
          </div>
        ) : (
          filtered.map((m) => (
            <div
              key={m.id}
              className="bg-surface border border-surface-border rounded-lg p-4 space-y-2 font-mono text-xs"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <span className="px-2 py-0.5 rounded bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 font-semibold uppercase text-[10px]">
                    {m.scope}
                  </span>
                  <span className="text-slate-200 font-bold">{m.key}</span>
                  <span className="text-slate-500 text-[10px]">({m.scope_id})</span>
                </div>
                <span className="text-[10px] text-slate-500">
                  {new Date(m.updated_at).toLocaleString()}
                </span>
              </div>
              <div className="p-3 bg-[#0a0d14] rounded border border-surface-border text-slate-300 whitespace-pre-wrap">
                {m.content}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
