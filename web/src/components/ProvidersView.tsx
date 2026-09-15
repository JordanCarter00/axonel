import React, { useEffect, useState } from 'react';
import { ProviderHealthInfo, AgentBackendInfo } from '../types';
import { api } from '../services/api';
import { Server, CheckCircle2, AlertTriangle, RefreshCw, Cpu, Bot } from 'lucide-react';

export const ProvidersView: React.FC = () => {
  const [providers, setProviders] = useState<Record<string, ProviderHealthInfo>>({});
  const [backends, setBackends] = useState<AgentBackendInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const loadData = async () => {
    setLoading(true);
    try {
      const [provData, backendData] = await Promise.all([
        api.listProviders().catch(() => ({})),
        api.listAgentBackends().catch(() => []),
      ]);
      setProviders(provData);
      setBackends(backendData);
    } catch (e) {
      console.error('Failed to load providers and backends:', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  return (
    <div className="space-y-8 pb-12">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Server className="w-5 h-5 text-indigo-400" />
            <span>LLM & Agent Host Infrastructure</span>
          </h1>
          <p className="text-xs text-slate-400">
            Real-time health of LLM providers and local external coding agent backends
          </p>
        </div>
        <button
          onClick={loadData}
          className="p-2 text-slate-400 hover:text-slate-200 bg-surface border border-surface-border rounded-md transition hover:border-indigo-500/50"
        >
          <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {/* LLM Providers */}
      <div className="space-y-3">
        <div className="flex items-center space-x-2">
          <Bot className="w-4 h-4 text-indigo-400" />
          <h2 className="text-sm font-semibold text-slate-200 uppercase tracking-wider">
            Inference Providers
          </h2>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {loading && Object.keys(providers).length === 0 ? (
            <div className="col-span-full p-8 text-center text-slate-400 font-mono text-xs">
              Querying provider status...
            </div>
          ) : Object.keys(providers).length === 0 ? (
            <div className="col-span-full p-8 text-center text-slate-400">
              No inference providers configured.
            </div>
          ) : (
            Object.entries(providers).map(([name, info]) => {
              const isHealthy = info.status === 'healthy';
              return (
                <div
                  key={name}
                  className="bg-surface border border-surface-border rounded-lg p-5 space-y-4"
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2.5">
                      {isHealthy ? (
                        <CheckCircle2 className="w-5 h-5 text-emerald-400" />
                      ) : (
                        <AlertTriangle className="w-5 h-5 text-rose-400" />
                      )}
                      <h3 className="font-semibold text-slate-100 text-sm">{name}</h3>
                    </div>
                    <span
                      className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${
                        isHealthy
                          ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                          : 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                      }`}
                    >
                      {info.status}
                    </span>
                  </div>

                  <div className="space-y-2 text-xs font-mono">
                    <div className="flex justify-between text-slate-400">
                      <span>Provider Type:</span>
                      <span className="text-slate-200">{info.provider_type}</span>
                    </div>
                    <div className="flex justify-between text-slate-400">
                      <span>Latency:</span>
                      <span className="text-slate-200">
                        {info.latency_ms !== undefined ? `${info.latency_ms} ms` : 'N/A'}
                      </span>
                    </div>
                    <div className="flex justify-between text-slate-400">
                      <span>Error Count:</span>
                      <span className={info.error_count ? 'text-rose-400' : 'text-slate-200'}>
                        {info.error_count ?? 0}
                      </span>
                    </div>
                    <div className="flex justify-between text-slate-400">
                      <span>Last Checked:</span>
                      <span className="text-slate-400 text-[11px]">
                        {info.last_checked ? new Date(info.last_checked).toLocaleTimeString() : 'N/A'}
                      </span>
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Local Agent Host Backends */}
      <div className="space-y-3 pt-4 border-t border-surface-border">
        <div className="flex items-center space-x-2">
          <Cpu className="w-4 h-4 text-cyan-400" />
          <h2 className="text-sm font-semibold text-slate-200 uppercase tracking-wider">
            Local Agent Host Backends (External Process Control)
          </h2>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {loading && backends.length === 0 ? (
            <div className="col-span-full p-8 text-center text-slate-400 font-mono text-xs">
              Querying agent backends...
            </div>
          ) : backends.length === 0 ? (
            <div className="col-span-full p-8 text-center text-slate-400">
              No external agent backends registered.
            </div>
          ) : (
            backends.map((backend) => (
              <div
                key={backend.id}
                className="bg-surface border border-surface-border rounded-lg p-5 space-y-4"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2.5">
                    <Cpu className={`w-5 h-5 ${backend.is_available ? 'text-cyan-400' : 'text-slate-500'}`} />
                    <div>
                      <h3 className="font-semibold text-slate-100 text-sm">{backend.display_name}</h3>
                      <span className="text-[10px] font-mono text-slate-400">{backend.id}</span>
                    </div>
                  </div>
                  <span
                    className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${
                      backend.is_available
                        ? 'bg-cyan-500/10 text-cyan-400 border-cyan-500/30'
                        : 'bg-slate-800 text-slate-400 border-slate-700'
                    }`}
                  >
                    {backend.is_available ? 'Ready' : 'Adapter Stub'}
                  </span>
                </div>

                <p className="text-xs text-slate-400 leading-relaxed">
                  {backend.description}
                </p>

                <div className="space-y-2 text-xs font-mono pt-2 border-t border-surface-border">
                  <div className="flex justify-between text-slate-400">
                    <span>Protocol:</span>
                    <span className="text-slate-200">v{backend.version}</span>
                  </div>
                  {backend.executable_path && (
                    <div className="text-slate-400">
                      <span className="block mb-0.5">Executable:</span>
                      <span className="text-slate-300 text-[10px] break-all bg-[#0a0d14] px-1.5 py-0.5 rounded block">
                        {backend.executable_path}
                      </span>
                    </div>
                  )}
                  {backend.capabilities && backend.capabilities.length > 0 && (
                    <div className="text-slate-400 pt-1">
                      <span className="block mb-1 text-[10px] uppercase font-semibold">Capabilities:</span>
                      <div className="flex flex-wrap gap-1">
                        {backend.capabilities.map((cap) => (
                          <span
                            key={cap}
                            className="px-1.5 py-0.5 text-[9px] font-mono rounded bg-slate-800/80 text-slate-300 border border-slate-700/60"
                          >
                            {cap}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
