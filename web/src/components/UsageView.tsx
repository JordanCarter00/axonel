import React, { useState, useEffect } from 'react';
import { ProviderCapabilities, PruneRetentionResponse } from '../types';
import { api } from '../services/api';

export const UsageView: React.FC = () => {
  const [capabilities, setCapabilities] = useState<ProviderCapabilities[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [pruning, setPruning] = useState<boolean>(false);
  const [pruneResult, setPruneResult] = useState<PruneRetentionResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadCapabilities();
  }, []);

  const loadCapabilities = async () => {
    try {
      setLoading(true);
      setError(null);
      const caps = await api.getProviderCapabilities();
      setCapabilities(caps);
    } catch (err: any) {
      setError(err.message || 'Failed to load provider capabilities');
    } finally {
      setLoading(false);
    }
  };

  const handlePrune = async () => {
    try {
      setPruning(true);
      setError(null);
      const res = await api.pruneRetentionRecords(30);
      setPruneResult(res);
    } catch (err: any) {
      setError(err.message || 'Failed to prune historical records');
    } finally {
      setPruning(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold text-slate-100">Provider Capabilities & Usage</h2>
          <p className="text-sm text-slate-400 mt-1">
            Authoritative capability matrix, token pricing, reasoning tiers, and retention policy management.
          </p>
        </div>
        <button
          onClick={handlePrune}
          disabled={pruning}
          className="px-4 py-2 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 text-xs font-semibold rounded-lg transition border border-slate-700 flex items-center space-x-2"
        >
          <svg className={`w-4 h-4 text-amber-400 ${pruning ? 'animate-spin' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          <span>{pruning ? 'Pruning...' : 'Prune Stale Records (30d+)'}</span>
        </button>
      </div>

      {pruneResult && (
        <div className="p-4 bg-emerald-950/40 border border-emerald-800/60 rounded-xl text-xs text-emerald-300 flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <span className="w-2 h-2 rounded-full bg-emerald-400"></span>
            <span>
              Retention enforced: <strong>{pruneResult.records_pruned}</strong> stale records pruned (events: {pruneResult.report.pruned_events}, messages: {pruneResult.report.pruned_messages}, commands: {pruneResult.report.pruned_commands})
            </span>
          </div>
          <span className="text-[11px] text-slate-500 font-mono">Cutoff: {new Date(pruneResult.cutoff_date).toLocaleDateString()}</span>
        </div>
      )}

      {error && (
        <div className="p-4 bg-red-950/40 border border-red-800/60 rounded-xl text-xs text-red-300">
          {error}
        </div>
      )}

      {/* Provider Capabilities Table */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
        <div className="px-6 py-4 border-b border-slate-800 bg-slate-950/50">
          <h3 className="text-sm font-semibold text-slate-200">Provider Capability Matrix & Pricing</h3>
        </div>

        {loading ? (
          <div className="py-12 text-center text-slate-400">Loading capability matrix...</div>
        ) : capabilities.length === 0 ? (
          <div className="py-12 text-center text-slate-400">No capabilities discovered.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950/80 text-slate-400 font-semibold border-b border-slate-800">
                <tr>
                  <th className="px-6 py-3">Provider</th>
                  <th className="px-6 py-3">Model</th>
                  <th className="px-6 py-3">Reasoning Tier</th>
                  <th className="px-6 py-3">Context Window</th>
                  <th className="px-6 py-3">Pricing (Prompt / Compl)</th>
                  <th className="px-6 py-3">Features</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 text-slate-300">
                {capabilities.map((cap, idx) => {
                  let tierColor = 'bg-blue-950/60 text-blue-300 border-blue-800/60';
                  const tierStr = String(cap.reasoning_tier || '').toLowerCase();
                  if (tierStr === 'high') {
                    tierColor = 'bg-purple-950/60 text-purple-300 border-purple-800/60';
                  } else if (tierStr === 'medium') {
                    tierColor = 'bg-sky-950/60 text-sky-300 border-sky-800/60';
                  } else if (tierStr === 'low' || tierStr === 'none') {
                    tierColor = 'bg-emerald-950/60 text-emerald-300 border-emerald-800/60';
                  }

                  const promptPer1M = ((cap.pricing?.cost_per_1k_input_tokens ?? 0) * 1000).toFixed(2);
                  const complPer1M = ((cap.pricing?.cost_per_1k_output_tokens ?? 0) * 1000).toFixed(2);

                  return (
                    <tr key={idx} className="hover:bg-slate-800/40 transition">
                      <td className="px-6 py-3 font-semibold uppercase tracking-wider text-[11px] text-slate-400">
                        {cap.provider}
                      </td>
                      <td className="px-6 py-3 font-mono font-medium text-slate-200">
                        {cap.model}
                      </td>
                      <td className="px-6 py-3">
                        <span className={`px-2 py-0.5 rounded border text-[10px] font-semibold uppercase ${tierColor}`}>
                          {cap.reasoning_tier}
                        </span>
                      </td>
                      <td className="px-6 py-3 font-mono">
                        {(cap.context_window_tokens ?? 0).toLocaleString()} tokens
                      </td>
                      <td className="px-6 py-3 font-mono text-slate-400">
                        ${promptPer1M} / ${complPer1M} <span className="text-[10px] text-slate-500">per 1M</span>
                      </td>
                      <td className="px-6 py-3">
                        <div className="flex items-center space-x-1.5">
                          {cap.supports_tools && (
                            <span className="px-1.5 py-0.5 bg-slate-800 text-slate-300 rounded text-[10px] border border-slate-700">
                              Tools
                            </span>
                          )}
                          {cap.supports_streaming && (
                            <span className="px-1.5 py-0.5 bg-slate-800 text-slate-300 rounded text-[10px] border border-slate-700">
                              Stream
                            </span>
                          )}
                          {cap.supports_vision && (
                            <span className="px-1.5 py-0.5 bg-slate-800 text-slate-300 rounded text-[10px] border border-slate-700">
                              Vision
                            </span>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
