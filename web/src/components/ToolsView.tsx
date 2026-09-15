import React, { useEffect, useState } from 'react';
import { ToolInfo } from '../types';
import { api } from '../services/api';
import { Wrench, Shield, RefreshCw } from 'lucide-react';

export const ToolsView: React.FC = () => {
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const loadTools = async () => {
    setLoading(true);
    try {
      const data = await api.listTools();
      setTools(data);
    } catch (e) {
      console.error('Failed to load tools:', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadTools();
  }, []);

  return (
    <div className="space-y-6 pb-12">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Wrench className="w-5 h-5 text-indigo-400" />
            <span>Sandbox Tool Registry</span>
          </h1>
          <p className="text-xs text-slate-400">
            Registered tools with sandbox isolation, telemetry audit, and secret redaction filters
          </p>
        </div>
        <button
          onClick={loadTools}
          className="p-2 text-slate-400 hover:text-slate-200 bg-surface border border-surface-border rounded-md"
        >
          <RefreshCw className="w-4 h-4" />
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {loading && tools.length === 0 ? (
          <div className="col-span-full p-12 text-center text-slate-400 font-mono text-xs">
            Loading tool definitions...
          </div>
        ) : tools.length === 0 ? (
          <div className="col-span-full p-12 text-center text-slate-400">
            No tools registered in current tool registry.
          </div>
        ) : (
          tools.map((tool) => (
            <div
              key={tool.name}
              className="bg-surface border border-surface-border rounded-lg p-4 space-y-3"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <div className="w-7 h-7 rounded bg-indigo-500/20 text-indigo-400 flex items-center justify-center font-mono font-bold text-xs">
                    T
                  </div>
                  <span className="font-mono font-semibold text-slate-200 text-sm">
                    {tool.name}
                  </span>
                </div>
                <span className="flex items-center space-x-1 text-[10px] font-mono uppercase px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">
                  <Shield className="w-2.5 h-2.5" />
                  <span>Sandboxed</span>
                </span>
              </div>

              <p className="text-xs text-slate-400">{tool.description}</p>

              <div className="space-y-1">
                <div className="text-[10px] font-semibold text-slate-400 uppercase tracking-wider">
                  Schema Definition:
                </div>
                <div className="bg-[#0a0d14] p-2.5 rounded font-mono text-xs text-slate-300 overflow-x-auto max-h-40 border border-surface-border">
                  <pre>{JSON.stringify(tool.schema, null, 2)}</pre>
                </div>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
