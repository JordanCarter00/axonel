import React, { useEffect, useState } from 'react';
import { ProviderHealthInfo } from '../types';
import { api } from '../services/api';
import { Server, CheckCircle2, AlertTriangle, RefreshCw } from 'lucide-react';

export const ProvidersView: React.FC = () => {
  const [providers, setProviders] = useState<Record<string, ProviderHealthInfo>>({});
  const [loading, setLoading] = useState(true);

  const loadProviders = async () => {
    setLoading(true);
    try {
      const data = await api.listProviders();
      setProviders(data);
    } catch (e) {
      console.error('Failed to load providers:', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadProviders();
  }, []);

  return (
    <div className="space-y-6 pb-12">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Server className="w-5 h-5 text-indigo-400" />
            <span>LLM & Tool Provider Health</span>
          </h1>
          <p className="text-xs text-slate-400">
            Real-time availability, latency profiling, and error failure monitoring across providers
          </p>
        </div>
        <button
          onClick={loadProviders}
          className="p-2 text-slate-400 hover:text-slate-200 bg-surface border border-surface-border rounded-md"
        >
          <RefreshCw className="w-4 h-4" />
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {loading && Object.keys(providers).length === 0 ? (
          <div className="col-span-full p-12 text-center text-slate-400 font-mono text-xs">
            Querying provider status...
          </div>
        ) : Object.keys(providers).length === 0 ? (
          <div className="col-span-full p-12 text-center text-slate-400">
            No provider backends configured.
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
  );
};
